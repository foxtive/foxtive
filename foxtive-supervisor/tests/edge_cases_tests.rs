mod common;
use foxtive_supervisor::Supervisor;
use foxtive_supervisor::task_pool::{LoadBalancingStrategy, TaskPool};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

// TASK GROUPS EDGE CASES

#[tokio::test]
async fn test_empty_group_operations() {
    struct SimpleTask {
        id: &'static str,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for SimpleTask {
        fn id(&self) -> &'static str {
            self.id
        }

        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(())
        }
    }

    let supervisor = Supervisor::new()
        .add(SimpleTask { id: "task-1" })
        .add(SimpleTask { id: "task-2" });

    let mut runtime = supervisor.start().await.unwrap();

    // Operations on non-existent groups should not panic
    let started = runtime.start_group("nonexistent-group");
    assert_eq!(started, 0);

    let stopped = runtime.stop_group("nonexistent-group").await;
    assert_eq!(stopped, 0);

    let restarted = runtime.restart_group("nonexistent-group");
    assert_eq!(restarted, 0);

    let tasks = runtime.list_group_tasks("nonexistent-group");
    assert_eq!(tasks.len(), 0);

    runtime.shutdown().await;
}

#[tokio::test]
async fn test_group_with_single_task() {
    struct SingleGroupTask {
        id: &'static str,
        group: &'static str,
        executed: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for SingleGroupTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn group_id(&self) -> Option<&'static str> {
            Some(self.group)
        }

        async fn run(&self) -> anyhow::Result<()> {
            self.executed.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let executed = Arc::new(AtomicUsize::new(0));

    let supervisor = Supervisor::new().add(SingleGroupTask {
        id: "solo-task",
        group: "single",
        executed: executed.clone(),
    });

    let mut runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Stop the single-task group
    let stopped = runtime.stop_group("single").await;
    assert_eq!(stopped, 1);

    // Verify task was executed before stop
    assert!(executed.load(Ordering::SeqCst) >= 1);
}

#[tokio::test]
async fn test_multiple_groups_overlap() {
    struct OverlapTask {
        id: &'static str,
        group: Option<&'static str>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for OverlapTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn group_id(&self) -> Option<&'static str> {
            self.group
        }

        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let supervisor = Supervisor::new()
        .add(OverlapTask {
            id: "t1",
            group: Some("group-a"),
        })
        .add(OverlapTask {
            id: "t2",
            group: Some("group-a"),
        })
        .add(OverlapTask {
            id: "t3",
            group: Some("group-b"),
        })
        .add(OverlapTask {
            id: "t4",
            group: Some("group-b"),
        })
        .add(OverlapTask {
            id: "t5",
            group: None,
        }); // No group

    let runtime = supervisor.start().await.unwrap();

    let group_a = runtime.list_group_tasks("group-a");
    assert_eq!(group_a.len(), 2);

    let group_b = runtime.list_group_tasks("group-b");
    assert_eq!(group_b.len(), 2);

    // Task without group shouldn't appear in any group
    let no_group = runtime.list_group_tasks("nonexistent");
    assert_eq!(no_group.len(), 0);

    runtime.shutdown().await;
}

// HEALTH AGGREGATION EDGE CASES

#[tokio::test]
async fn test_group_health_empty_group() {
    struct HealthTask {
        id: &'static str,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for HealthTask {
        fn id(&self) -> &'static str {
            self.id
        }

        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let supervisor = Supervisor::new().add(HealthTask { id: "task-1" });

    let runtime = supervisor.start().await.unwrap();

    // Empty group should return Unknown health
    let health = runtime.get_group_health("empty-group").await;
    match health {
        foxtive_supervisor::enums::HealthStatus::Unknown => {}
        _ => panic!("Expected Unknown health for empty group"),
    }

    runtime.shutdown().await;
}

#[tokio::test]
async fn test_group_health_all_healthy() {
    use foxtive_supervisor::enums::HealthStatus;

    struct HealthyTask {
        id: &'static str,
        group: &'static str,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for HealthyTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn group_id(&self) -> Option<&'static str> {
            Some(self.group)
        }

        async fn health_check(&self) -> HealthStatus {
            HealthStatus::Healthy
        }

        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let supervisor = Supervisor::new()
        .add(HealthyTask {
            id: "h1",
            group: "all-healthy",
        })
        .add(HealthyTask {
            id: "h2",
            group: "all-healthy",
        })
        .add(HealthyTask {
            id: "h3",
            group: "all-healthy",
        });

    let runtime = supervisor.start().await.unwrap();

    let health = runtime.get_group_health("all-healthy").await;
    match health {
        HealthStatus::Healthy => {}
        _ => panic!("Expected Healthy, got {:?}", health),
    }

    runtime.shutdown().await;
}

#[tokio::test]
async fn test_group_health_mixed_statuses() {
    use foxtive_supervisor::enums::HealthStatus;

    struct MixedHealthTask {
        id: &'static str,
        group: &'static str,
        status: HealthStatus,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for MixedHealthTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn group_id(&self) -> Option<&'static str> {
            Some(self.group)
        }

        async fn health_check(&self) -> HealthStatus {
            self.status.clone()
        }

        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let supervisor = Supervisor::new()
        .add(MixedHealthTask {
            id: "healthy",
            group: "mixed",
            status: HealthStatus::Healthy,
        })
        .add(MixedHealthTask {
            id: "degraded",
            group: "mixed",
            status: HealthStatus::Degraded {
                reason: "Slow response".to_string(),
            },
        })
        .add(MixedHealthTask {
            id: "unhealthy",
            group: "mixed",
            status: HealthStatus::Unhealthy {
                reason: "Connection failed".to_string(),
            },
        });

    let runtime = supervisor.start().await.unwrap();

    // Should return worst status (Unhealthy)
    let health = runtime.get_group_health("mixed").await;
    match health {
        HealthStatus::Unhealthy { .. } => {}
        _ => panic!("Expected Unhealthy (worst case), got {:?}", health),
    }

    runtime.shutdown().await;
}

// TASK POOL EDGE CASES

#[tokio::test]
async fn test_pool_size_one() {
    #[allow(dead_code)]
    struct PoolWorker {
        id: &'static str,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for PoolWorker {
        fn id(&self) -> &'static str {
            self.id
        }

        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let pool = TaskPool::new("tiny-pool", 1, LoadBalancingStrategy::RoundRobin);

    // Round-robin with size 1 should always return 0
    for _ in 0..10 {
        assert_eq!(pool.get_next_worker().await, 0);
    }
}

#[tokio::test]
async fn test_pool_zero_size_handled() {
    // Builder should enforce minimum size of 1
    let pool = foxtive_supervisor::task_pool::TaskPoolBuilder::new("min-pool")
        .with_size(0) // Should be clamped to 1
        .build();

    assert_eq!(pool.pool_size, 1);
}

#[tokio::test]
async fn test_pool_large_size() {
    let pool = TaskPool::new("large-pool", 100, LoadBalancingStrategy::RoundRobin);

    // Should cycle through all 100 workers
    for i in 0..200 {
        let worker = pool.get_next_worker().await;
        assert_eq!(worker, i % 100);
    }
}

// CONDITIONAL DEPENDENCIES EDGE CASES

#[tokio::test]
async fn test_conditional_dep_always_false() {
    struct NeverDepTask {
        id: &'static str,
        executed: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for NeverDepTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn conditional_dependencies(
            &self,
        ) -> Vec<(&'static str, Box<dyn Fn() -> bool + Send + Sync>)> {
            vec![("nonexistent", Box::new(|| false))] // Always false
        }

        async fn run(&self) -> anyhow::Result<()> {
            self.executed.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let executed = Arc::new(AtomicUsize::new(0));

    // Task should run even though dependency doesn't exist (condition is false)
    let supervisor = Supervisor::new().add(NeverDepTask {
        id: "independent",
        executed: executed.clone(),
    });

    let runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    runtime.shutdown().await;

    assert_eq!(executed.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_conditional_dep_nonexistent_when_true() {
    struct BadCondTask {
        id: &'static str,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for BadCondTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn conditional_dependencies(
            &self,
        ) -> Vec<(&'static str, Box<dyn Fn() -> bool + Send + Sync>)> {
            vec![("does-not-exist", Box::new(|| true))] // True but dep doesn't exist
        }

        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    // This should fail during startup because the conditional dependency
    // evaluates to true but the task doesn't exist
    let supervisor = Supervisor::new().add(BadCondTask { id: "bad-dep" });

    let result = supervisor.start().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_many_conditional_dependencies() {
    struct ManyCondTask {
        id: &'static str,
        setup_order: Arc<std::sync::Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for ManyCondTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn conditional_dependencies(
            &self,
        ) -> Vec<(&'static str, Box<dyn Fn() -> bool + Send + Sync>)> {
            if self.id == "main" {
                // Multiple conditional dependencies
                vec![
                    ("dep-1", Box::new(|| true)),
                    ("dep-2", Box::new(|| true)),
                    ("dep-3", Box::new(|| true)),
                    ("dep-4", Box::new(|| false)), // This one won't be active
                ]
            } else {
                vec![]
            }
        }

        async fn setup(&self) -> anyhow::Result<()> {
            let mut order = self.setup_order.lock().unwrap();
            order.push(self.id.to_string());
            Ok(())
        }

        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let setup_order = Arc::new(std::sync::Mutex::new(Vec::new()));

    let supervisor = Supervisor::new()
        .add(ManyCondTask {
            id: "dep-1",
            setup_order: setup_order.clone(),
        })
        .add(ManyCondTask {
            id: "dep-2",
            setup_order: setup_order.clone(),
        })
        .add(ManyCondTask {
            id: "dep-3",
            setup_order: setup_order.clone(),
        })
        .add(ManyCondTask {
            id: "dep-4",
            setup_order: setup_order.clone(),
        }) // Won't be required
        .add(ManyCondTask {
            id: "main",
            setup_order: setup_order.clone(),
        });

    let runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    runtime.shutdown().await;

    let order = setup_order.lock().unwrap();
    // main should setup after dep-1, dep-2, dep-3 (but not necessarily dep-4)
    let main_pos = order.iter().position(|x| x == "main").unwrap();
    let dep1_pos = order.iter().position(|x| x == "dep-1").unwrap();
    let dep2_pos = order.iter().position(|x| x == "dep-2").unwrap();
    let dep3_pos = order.iter().position(|x| x == "dep-3").unwrap();

    assert!(dep1_pos < main_pos);
    assert!(dep2_pos < main_pos);
    assert!(dep3_pos < main_pos);
}

// HIERARCHY EDGE CASES
// Note: Hierarchy tests moved to hierarchy_integration_tests.rs

// COMBINED FEATURES EDGE CASES

#[tokio::test]
async fn test_group_with_conditional_deps() {
    struct GroupCondTask {
        id: &'static str,
        group: &'static str,
        has_cond_dep: bool,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for GroupCondTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn group_id(&self) -> Option<&'static str> {
            Some(self.group)
        }

        fn conditional_dependencies(
            &self,
        ) -> Vec<(&'static str, Box<dyn Fn() -> bool + Send + Sync>)> {
            if self.has_cond_dep {
                vec![("optional-dep", Box::new(|| false))] // Inactive
            } else {
                vec![]
            }
        }

        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let supervisor = Supervisor::new()
        .add(GroupCondTask {
            id: "opt-dep",
            group: "my-group",
            has_cond_dep: false,
        })
        .add(GroupCondTask {
            id: "main-task",
            group: "my-group",
            has_cond_dep: true,
        });

    let runtime = supervisor.start().await.unwrap();

    // Verify group operations work with conditional deps
    let group_tasks = runtime.list_group_tasks("my-group");
    assert_eq!(group_tasks.len(), 2);

    runtime.shutdown().await;
}

#[tokio::test]
async fn test_rapid_group_stop_start() {
    struct RapidTask {
        id: &'static str,
        group: &'static str,
        count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for RapidTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn group_id(&self) -> Option<&'static str> {
            Some(self.group)
        }

        async fn run(&self) -> anyhow::Result<()> {
            self.count.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(())
        }
    }

    let count = Arc::new(AtomicUsize::new(0));

    let supervisor = Supervisor::new()
        .add(RapidTask {
            id: "r1",
            group: "rapid",
            count: count.clone(),
        })
        .add(RapidTask {
            id: "r2",
            group: "rapid",
            count: count.clone(),
        });

    let mut runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Stop and restart rapidly
    runtime.stop_group("rapid").await;
    runtime.start_group("rapid");

    tokio::time::sleep(Duration::from_millis(150)).await;

    runtime.shutdown().await;

    // Tasks should have executed at least once
    assert!(count.load(Ordering::SeqCst) >= 2);
}
