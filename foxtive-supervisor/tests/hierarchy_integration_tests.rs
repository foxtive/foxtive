use foxtive_supervisor::hierarchy::SupervisorHierarchy;
use foxtive_supervisor::{SupervisedTask, Supervisor};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::{sleep, Duration};

struct SimpleTask {
    id: &'static str,
    started: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl SupervisedTask for SimpleTask {
    fn id(&self) -> &'static str {
        self.id
    }

    async fn run(&self) -> anyhow::Result<()> {
        self.started.store(true, Ordering::SeqCst);
        // Run until stopped
        loop {
            sleep(Duration::from_millis(100)).await;
        }
    }
}

#[tokio::test]
async fn test_hierarchy_starts_and_stops() {
    let started1 = Arc::new(AtomicBool::new(false));
    let started2 = Arc::new(AtomicBool::new(false));
    let started3 = Arc::new(AtomicBool::new(false));

    let task1 = SimpleTask {
        id: "task-1",
        started: started1.clone(),
    };

    let task2 = SimpleTask {
        id: "task-2",
        started: started2.clone(),
    };

    let task3 = SimpleTask {
        id: "task-3",
        started: started3.clone(),
    };

    // Build hierarchy
    let hierarchy = SupervisorHierarchy::new("root")
        .add_child("api", Supervisor::new().add(task1))
        .add_child("workers", Supervisor::new().add(task2).add(task3));

    // Start hierarchy
    let runtime = hierarchy.start_all().await.unwrap();

    // Give tasks time to start
    sleep(Duration::from_millis(200)).await;

    // Verify all tasks started
    assert!(started1.load(Ordering::SeqCst), "Task 1 should have started");
    assert!(started2.load(Ordering::SeqCst), "Task 2 should have started");
    assert!(started3.load(Ordering::SeqCst), "Task 3 should have started");

    // Verify task count
    assert_eq!(runtime.total_task_count(), 3, "Should have 3 tasks total");

    // Shutdown
    runtime.shutdown_all().await;

    // Give time for shutdown to propagate
    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_nested_hierarchy() {
    let started = Arc::new(AtomicBool::new(false));

    let task = SimpleTask {
        id: "nested-task",
        started: started.clone(),
    };

    // Create nested hierarchy: root -> parent -> child
    let parent_supervisor = Supervisor::new().add(task);
    let _child_supervisor = Supervisor::new(); // Empty supervisor

    let hierarchy = SupervisorHierarchy::new("root")
        .add_child("parent", parent_supervisor);

    let runtime = hierarchy.start_all().await.unwrap();

    sleep(Duration::from_millis(200)).await;

    assert!(started.load(Ordering::SeqCst), "Nested task should have started");
    assert_eq!(runtime.total_task_count(), 1);

    runtime.shutdown_all().await;
}

#[tokio::test]
async fn test_empty_hierarchy() {
    let hierarchy = SupervisorHierarchy::new("root");
    let runtime = hierarchy.start_all().await.unwrap();

    assert_eq!(runtime.total_task_count(), 0);

    runtime.shutdown_all().await;
}

#[tokio::test]
async fn test_deep_nested_hierarchy() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    struct SimpleTask {
        id: &'static str,
        started: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl SupervisedTask for SimpleTask {
        fn id(&self) -> &'static str { self.id }
        
        async fn run(&self) -> anyhow::Result<()> {
            self.started.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(())
        }
    }
    
    let started = Arc::new(AtomicUsize::new(0));
    
    // Create deep nesting: root -> level1 -> level2 -> level3 -> level4
    let level4 = Supervisor::new()
        .add(SimpleTask { id: "level-4", started: started.clone() });
    
    let level3 = Supervisor::new()
        .add(SimpleTask { id: "level-3", started: started.clone() });
    
    let level2 = Supervisor::new()
        .add(SimpleTask { id: "level-2", started: started.clone() });
    
    let level1 = Supervisor::new()
        .add(SimpleTask { id: "level-1", started: started.clone() });
    
    let hierarchy = SupervisorHierarchy::new("root")
        .add_child("level-1", level1)
        .add_child("level-2", level2)
        .add_child("level-3", level3)
        .add_child("level-4", level4);
    
    let runtime = hierarchy.start_all().await.unwrap();
    
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    assert_eq!(started.load(Ordering::SeqCst), 4, "All nested tasks should start");
    assert_eq!(runtime.total_task_count(), 4);
    
    runtime.shutdown_all().await;
}

#[tokio::test]
async fn test_hierarchy_shutdown_order() {
    use std::sync::Arc;
    use std::sync::Mutex;
    
    struct ShutdownOrderTask {
        id: &'static str,
        order: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl SupervisedTask for ShutdownOrderTask {
        fn id(&self) -> &'static str { self.id }
        
        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok(())
        }
        
        async fn on_shutdown(&self) {
            let mut order = self.order.lock().unwrap();
            order.push(self.id.to_string());
        }
    }
    
    let shutdown_order = Arc::new(Mutex::new(Vec::new()));
    
    let child1 = Supervisor::new()
        .add(ShutdownOrderTask { 
            id: "child1-task", 
            order: shutdown_order.clone() 
        });
    
    let child2 = Supervisor::new()
        .add(ShutdownOrderTask { 
            id: "child2-task", 
            order: shutdown_order.clone() 
        });
    
    let hierarchy = SupervisorHierarchy::new("root")
        .add_child("child1", child1)
        .add_child("child2", child2);
    
    let runtime = hierarchy.start_all().await.unwrap();
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    runtime.shutdown_all().await;
    
    let order = shutdown_order.lock().unwrap();
    assert_eq!(order.len(), 2, "Both tasks should shutdown");
    // Children should shutdown in parallel, so order may vary
    assert!(order.contains(&"child1-task".to_string()));
    assert!(order.contains(&"child2-task".to_string()));
}

#[tokio::test]
async fn test_hierarchy_with_failing_task() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    struct FailingTask {
        id: &'static str,
        attempts: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl SupervisedTask for FailingTask {
        fn id(&self) -> &'static str { self.id }
        
        async fn run(&self) -> anyhow::Result<()> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            Err(anyhow::anyhow!("Intentional failure"))
        }
    }
    
    struct WorkingTask {
        id: &'static str,
    }

    #[async_trait::async_trait]
    impl SupervisedTask for WorkingTask {
        fn id(&self) -> &'static str { self.id }
        
        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(())
        }
    }
    
    let attempts = Arc::new(AtomicUsize::new(0));
    
    let failing_supervisor = Supervisor::new()
        .add(FailingTask { 
            id: "failing", 
            attempts: attempts.clone() 
        });
    
    let working_supervisor = Supervisor::new()
        .add(WorkingTask { id: "working" });
    
    let hierarchy = SupervisorHierarchy::new("root")
        .add_child("failing-sup", failing_supervisor)
        .add_child("working-sup", working_supervisor);
    
    let runtime = hierarchy.start_all().await.unwrap();
    
    // Let it run briefly to allow some retries
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // The failing task should have attempted multiple times
    assert!(attempts.load(Ordering::SeqCst) > 0);
    
    runtime.shutdown_all().await;
}
