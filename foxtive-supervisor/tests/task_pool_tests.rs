mod common;
use foxtive_supervisor::task_pool::{TaskPool, TaskPoolBuilder, LoadBalancingStrategy};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[tokio::test]
async fn test_task_pool_creation() {
    struct WorkerTask {
        id: &'static str,
        #[allow(dead_code)]
        worker_index: usize,
        execution_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for WorkerTask {
        fn id(&self) -> &'static str { self.id }
        
        async fn run(&self) -> anyhow::Result<()> {
            self.execution_count.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(())
        }
    }

    let execution_count = Arc::new(AtomicUsize::new(0));
    
    // Create a pool of 3 workers
    let supervisor = TaskPoolBuilder::new("worker-pool")
        .with_size(3)
        .with_strategy(LoadBalancingStrategy::RoundRobin)
        .build_and_create(|index| {
            let ids = ["worker-0", "worker-1", "worker-2"];
            WorkerTask {
                id: ids[index],
                worker_index: index,
                execution_count: execution_count.clone(),
            }
        });
    
    let runtime = supervisor.start().await.unwrap();
    
    // Give tasks time to execute
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    runtime.shutdown().await;
    
    // All 3 workers should have executed
    let count = execution_count.load(Ordering::SeqCst);
    assert_eq!(count, 3, "All 3 workers should have executed");
}

#[tokio::test]
async fn test_pool_with_different_strategies() {
    #[allow(dead_code)]
    struct SimpleTask {
        id: &'static str,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for SimpleTask {
        fn id(&self) -> &'static str { self.id }
        
        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    // Test Round Robin
    let pool_rr = TaskPool::new("rr-pool", 3, LoadBalancingStrategy::RoundRobin);
    let info_rr = pool_rr.info();
    assert_eq!(info_rr.pool_size, 3);
    
    // Test Random
    let pool_random = TaskPool::new("random-pool", 5, LoadBalancingStrategy::Random);
    let info_random = pool_random.info();
    assert_eq!(info_random.pool_size, 5);
    
    // Test Least Loaded (falls back to round-robin for now)
    let pool_ll = TaskPool::new("ll-pool", 2, LoadBalancingStrategy::LeastLoaded);
    let info_ll = pool_ll.info();
    assert_eq!(info_ll.pool_size, 2);
}

#[tokio::test]
async fn test_pool_load_distribution() {
    struct CountingTask {
        id: &'static str,
        run_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for CountingTask {
        fn id(&self) -> &'static str { self.id }
        
        async fn run(&self) -> anyhow::Result<()> {
            self.run_count.fetch_add(1, Ordering::SeqCst);
            // Run once and complete
            Ok(())
        }
    }

    let total_runs = Arc::new(AtomicUsize::new(0));
    let pool_size = 5;
    
    let supervisor = TaskPoolBuilder::new("counting-pool")
        .with_size(pool_size)
        .build_and_create(|index| {
            let ids = ["counter-0", "counter-1", "counter-2", "counter-3", "counter-4"];
            CountingTask {
                id: ids[index],
                run_count: total_runs.clone(),
            }
        });
    
    let runtime = supervisor.start().await.unwrap();
    
    // Wait for all tasks to complete
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    runtime.shutdown().await;
    
    // Each worker should have run once
    let runs = total_runs.load(Ordering::SeqCst);
    assert_eq!(runs, pool_size, "Each worker in the pool should have run once");
}

#[tokio::test]
async fn test_pool_integration_with_groups() {
    use foxtive_supervisor::Supervisor;
    
    struct GroupedWorker {
        id: &'static str,
        group: &'static str,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for GroupedWorker {
        fn id(&self) -> &'static str { self.id }
        
        fn group_id(&self) -> Option<&'static str> {
            Some(self.group)
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    // Create a pool where all workers belong to the same group
    let mut supervisor = Supervisor::new();
    
    supervisor = supervisor.add(GroupedWorker {
        id: "pool-worker-0",
        group: "worker-pool",
    });
    supervisor = supervisor.add(GroupedWorker {
        id: "pool-worker-1",
        group: "worker-pool",
    });
    supervisor = supervisor.add(GroupedWorker {
        id: "pool-worker-2",
        group: "worker-pool",
    });
    supervisor = supervisor.add(GroupedWorker {
        id: "pool-worker-3",
        group: "worker-pool",
    });
    
    let runtime = supervisor.start().await.unwrap();
    
    // Verify all tasks are in the same group
    let group_tasks = runtime.list_group_tasks("worker-pool");
    assert_eq!(group_tasks.len(), 4, "All pool workers should be in the same group");
    
    runtime.shutdown().await;
}

#[tokio::test]
async fn test_large_pool_stress() {
    struct StressTask {
        id: &'static str,
        execution_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for StressTask {
        fn id(&self) -> &'static str { self.id }
        
        async fn run(&self) -> anyhow::Result<()> {
            self.execution_count.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(5)).await;
            Ok(())
        }
    }

    let execution_count = Arc::new(AtomicUsize::new(0));
    let pool_size = 20;
    
    // Create a large pool with static IDs
    let mut supervisor = foxtive_supervisor::Supervisor::new();
    
    for i in 0..pool_size {
        let id: &'static str = Box::leak(format!("stress-worker-{}", i).into_boxed_str());
        supervisor = supervisor.add(StressTask {
            id,
            execution_count: execution_count.clone(),
        });
    }
    
    let runtime = supervisor.start().await.unwrap();
    
    // Give tasks time to execute
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    runtime.shutdown().await;
    
    let count = execution_count.load(Ordering::SeqCst);
    assert_eq!(count, pool_size, "All {} workers should have executed", pool_size);
}

#[tokio::test]
async fn test_pool_concurrent_start_stop() {
    struct ConcurrentTask {
        id: &'static str,
        started: Arc<AtomicUsize>,
        stopped: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for ConcurrentTask {
        fn id(&self) -> &'static str { self.id }
        
        async fn run(&self) -> anyhow::Result<()> {
            self.started.fetch_add(1, Ordering::SeqCst);
            // Run indefinitely until stopped
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok(())
        }
        
        async fn on_shutdown(&self) {
            self.stopped.fetch_add(1, Ordering::SeqCst);
        }
    }

    let started = Arc::new(AtomicUsize::new(0));
    let stopped = Arc::new(AtomicUsize::new(0));
    
    let mut supervisor = foxtive_supervisor::Supervisor::new();
    
    for i in 0..10 {
        let ids = ["conc-0", "conc-1", "conc-2", "conc-3", "conc-4",
                   "conc-5", "conc-6", "conc-7", "conc-8", "conc-9"];
        supervisor = supervisor.add(ConcurrentTask {
            id: ids[i],
            started: started.clone(),
            stopped: stopped.clone(),
        });
    }
    
    let runtime = supervisor.start().await.unwrap();
    
    // Let them start
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    assert_eq!(started.load(Ordering::SeqCst), 10);
    
    // Shutdown all concurrently
    runtime.shutdown().await;
    
    assert_eq!(stopped.load(Ordering::SeqCst), 10, "All tasks should shutdown cleanly");
}

#[tokio::test]
async fn test_pool_with_failing_workers() {
    struct FailingWorker {
        id: &'static str,
        worker_index: usize,
        failure_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for FailingWorker {
        fn id(&self) -> &'static str { self.id }
        
        async fn run(&self) -> anyhow::Result<()> {
            if self.worker_index % 2 == 0 {
                // Even workers fail
                self.failure_count.fetch_add(1, Ordering::SeqCst);
                Err(anyhow::anyhow!("Worker {} intentionally failing", self.worker_index))
            } else {
                // Odd workers succeed
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok(())
            }
        }
        
        fn restart_policy(&self) -> foxtive_supervisor::enums::RestartPolicy {
            if self.worker_index % 2 == 0 {
                foxtive_supervisor::enums::RestartPolicy::MaxAttempts(2)
            } else {
                foxtive_supervisor::enums::RestartPolicy::Always
            }
        }
    }

    let failure_count = Arc::new(AtomicUsize::new(0));
    
    let supervisor = TaskPoolBuilder::new("mixed-pool")
        .with_size(4)
        .build_and_create(|index| {
            let ids = ["fail-0", "ok-1", "fail-2", "ok-3"];
            FailingWorker {
                id: ids[index],
                worker_index: index,
                failure_count: failure_count.clone(),
            }
        });
    
    let runtime = supervisor.start().await.unwrap();
    
    // Let it run briefly
    tokio::time::sleep(Duration::from_millis(300)).await;
    
    runtime.shutdown().await;
    
    // Should have some failures from even workers
    assert!(failure_count.load(Ordering::SeqCst) > 0);
}

#[tokio::test]
async fn test_pool_dynamic_scaling_simulation() {
    use std::sync::Mutex;
    
    struct ScalableTask {
        id: &'static str,
        active_tasks: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for ScalableTask {
        fn id(&self) -> &'static str { self.id }
        
        async fn run(&self) -> anyhow::Result<()> {
            {
                let mut active = self.active_tasks.lock().unwrap();
                active.push(self.id.to_string());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let active_tasks = Arc::new(Mutex::new(Vec::new()));
    
    // Start with small pool
    let mut supervisor = foxtive_supervisor::Supervisor::new();
    
    let task_ids = ["scale-task-0", "scale-task-1", "scale-task-2"];
    for id in &task_ids {
        supervisor = supervisor.add(ScalableTask {
            id: id,
            active_tasks: active_tasks.clone(),
        });
    }
    
    let runtime = supervisor.start().await.unwrap();
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    {
        let active = active_tasks.lock().unwrap();
        assert_eq!(active.len(), 3, "Initial pool should have 3 tasks");
    }
    
    runtime.shutdown().await;
}
