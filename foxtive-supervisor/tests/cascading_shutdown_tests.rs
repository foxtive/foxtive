mod common;
use foxtive_supervisor::hierarchy::SupervisorHierarchy;
use foxtive_supervisor::{SupervisedTask, Supervisor};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_cascading_shutdown_hierarchy() {
    struct ShutdownTask {
        id: &'static str,
        shutdown_order: Arc<std::sync::Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl SupervisedTask for ShutdownTask {
        fn id(&self) -> &'static str {
            self.id
        }

        async fn run(&self) -> anyhow::Result<()> {
            // Run indefinitely until stopped
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok(())
        }

        async fn on_shutdown(&self) {
            let mut order = self.shutdown_order.lock().unwrap();
            order.push(self.id.to_string());
        }
    }

    let shutdown_order = Arc::new(std::sync::Mutex::new(Vec::new()));

    // Create parent supervisor
    let parent = Supervisor::new().add(ShutdownTask {
        id: "parent-task",
        shutdown_order: shutdown_order.clone(),
    });

    // Create child supervisor
    let child = Supervisor::new().add(ShutdownTask {
        id: "child-task",
        shutdown_order: shutdown_order.clone(),
    });

    // Build and start hierarchy
    let hierarchy = SupervisorHierarchy::new("root")
        .add_child("parent", parent)
        .add_child("child", child);

    let runtime = hierarchy.start_all().await.unwrap();

    // Give tasks time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Shutdown - children should shutdown in parallel
    runtime.shutdown_all().await;

    // Verify both tasks shut down
    let order = shutdown_order.lock().unwrap();
    assert_eq!(order.len(), 2, "Both tasks should have shut down");
    assert!(order.contains(&"parent-task".to_string()));
    assert!(order.contains(&"child-task".to_string()));
}

#[tokio::test]
async fn test_hierarchy_with_nested_structure() {
    struct SimpleTask {
        id: &'static str,
    }

    #[async_trait::async_trait]
    impl SupervisedTask for SimpleTask {
        fn id(&self) -> &'static str {
            self.id
        }

        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    // Create nested structure: root -> level1 -> level2
    let level2 = Supervisor::new().add(SimpleTask { id: "level-2-task" });

    let level1 = Supervisor::new().add(SimpleTask { id: "level-1-task" });

    let hierarchy = SupervisorHierarchy::new("root")
        .add_child("level-1", level1)
        .add_child("level-2", level2);

    let runtime = hierarchy.start_all().await.unwrap();

    // Verify task count
    assert_eq!(runtime.total_task_count(), 2);

    runtime.shutdown_all().await;
}
