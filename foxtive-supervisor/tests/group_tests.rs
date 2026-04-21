mod common;
use foxtive_supervisor::Supervisor;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[tokio::test]
async fn test_task_groups_start_stop() {
    struct GroupTask {
        id: &'static str,
        group: &'static str,
        run_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for GroupTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn group_id(&self) -> Option<&'static str> {
            Some(self.group)
        }

        async fn run(&self) -> anyhow::Result<()> {
            self.run_count.fetch_add(1, Ordering::SeqCst);
            // Run for a while so we can stop it
            tokio::time::sleep(Duration::from_secs(10)).await;
            Ok(())
        }
    }

    let count_a = Arc::new(AtomicUsize::new(0));
    let count_b = Arc::new(AtomicUsize::new(0));

    let supervisor = Supervisor::new()
        .add(GroupTask {
            id: "task-a1",
            group: "group-a",
            run_count: count_a.clone(),
        })
        .add(GroupTask {
            id: "task-a2",
            group: "group-a",
            run_count: count_a.clone(),
        })
        .add(GroupTask {
            id: "task-b1",
            group: "group-b",
            run_count: count_b.clone(),
        });

    let mut runtime = supervisor.start().await.unwrap();

    // Give tasks time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify all tasks are running
    assert_eq!(
        count_a.load(Ordering::SeqCst),
        2,
        "Both group-a tasks should be running"
    );
    assert_eq!(
        count_b.load(Ordering::SeqCst),
        1,
        "Group-b task should be running"
    );

    // Stop group-a
    let stopped = runtime.stop_group("group-a").await;
    assert_eq!(stopped, 2, "Should have stopped 2 tasks in group-a");

    // Give time for shutdown
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Group-b should still be running
    assert_eq!(
        count_b.load(Ordering::SeqCst),
        1,
        "Group-b should still be running"
    );

    runtime.shutdown().await;
}

#[tokio::test]
async fn test_list_group_tasks() {
    struct SimpleGroupTask {
        id: &'static str,
        group: &'static str,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for SimpleGroupTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn group_id(&self) -> Option<&'static str> {
            Some(self.group)
        }

        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(())
        }
    }

    let supervisor = Supervisor::new()
        .add(SimpleGroupTask {
            id: "db-conn",
            group: "database",
        })
        .add(SimpleGroupTask {
            id: "db-pool",
            group: "database",
        })
        .add(SimpleGroupTask {
            id: "api-server",
            group: "api",
        });

    let runtime = supervisor.start().await.unwrap();

    // List tasks in database group
    let db_tasks = runtime.list_group_tasks("database");
    assert_eq!(db_tasks.len(), 2);
    assert!(db_tasks.contains(&"db-conn".to_string()));
    assert!(db_tasks.contains(&"db-pool".to_string()));

    // List tasks in api group
    let api_tasks = runtime.list_group_tasks("api");
    assert_eq!(api_tasks.len(), 1);
    assert!(api_tasks.contains(&"api-server".to_string()));

    runtime.shutdown().await;
}

#[tokio::test]
async fn test_restart_group() {
    struct RestartableGroupTask {
        id: &'static str,
        group: &'static str,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for RestartableGroupTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn group_id(&self) -> Option<&'static str> {
            Some(self.group)
        }

        async fn run(&self) -> anyhow::Result<()> {
            // Task runs once and completes
            Ok(())
        }
    }

    let supervisor = Supervisor::new()
        .add(RestartableGroupTask {
            id: "worker-1",
            group: "workers",
        })
        .add(RestartableGroupTask {
            id: "worker-2",
            group: "workers",
        });

    let runtime = supervisor.start().await.unwrap();

    // Give tasks time to start and complete
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Restart the group - this should send restart messages even if tasks completed
    let restarted = runtime.restart_group("workers");
    assert_eq!(restarted, 2, "Should have sent restart to 2 tasks");

    runtime.shutdown().await;
}

#[tokio::test]
async fn test_group_health_aggregation() {
    use foxtive_supervisor::enums::HealthStatus;

    struct HealthTask {
        id: &'static str,
        group: &'static str,
        status: HealthStatus,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for HealthTask {
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
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(())
        }
    }

    let supervisor = Supervisor::new()
        .add(HealthTask {
            id: "healthy-1",
            group: "group-a",
            status: HealthStatus::Healthy,
        })
        .add(HealthTask {
            id: "healthy-2",
            group: "group-a",
            status: HealthStatus::Healthy,
        })
        .add(HealthTask {
            id: "degraded-1",
            group: "group-b",
            status: HealthStatus::Degraded {
                reason: "Test degradation".to_string(),
            },
        })
        .add(HealthTask {
            id: "healthy-3",
            group: "group-b",
            status: HealthStatus::Healthy,
        })
        .add(HealthTask {
            id: "unhealthy-1",
            group: "group-c",
            status: HealthStatus::Unhealthy {
                reason: "Test failure".to_string(),
            },
        })
        .add(HealthTask {
            id: "healthy-4",
            group: "group-c",
            status: HealthStatus::Healthy,
        });

    let runtime = supervisor.start().await.unwrap();

    // Test all healthy group
    let health_a = runtime.get_group_health("group-a").await;
    assert_eq!(health_a, HealthStatus::Healthy);

    // Test mixed degraded group (worst status wins)
    let health_b = runtime.get_group_health("group-b").await;
    match health_b {
        HealthStatus::Degraded { .. } => {} // Expected
        _ => panic!("Expected Degraded, got {:?}", health_b),
    }

    // Test mixed unhealthy group (worst status wins)
    let health_c = runtime.get_group_health("group-c").await;
    match health_c {
        HealthStatus::Unhealthy { .. } => {} // Expected
        _ => panic!("Expected Unhealthy, got {:?}", health_c),
    }

    // Test non-existent group
    let health_unknown = runtime.get_group_health("nonexistent").await;
    assert_eq!(health_unknown, HealthStatus::Unknown);

    // Test detailed health info
    let details = runtime.get_group_health_details("group-b").await;
    assert_eq!(details.len(), 2);

    runtime.shutdown().await;
}
