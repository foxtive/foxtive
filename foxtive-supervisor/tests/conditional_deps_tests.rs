mod common;
use foxtive_supervisor::Supervisor;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[tokio::test]
async fn test_conditional_dependencies_enabled() {
    struct ConditionalTask {
        id: &'static str,
        setup_order: Arc<std::sync::Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for ConditionalTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn conditional_dependencies(
            &self,
        ) -> Vec<(&'static str, Box<dyn Fn() -> bool + Send + Sync>)> {
            if self.id == "dependent-task" {
                vec![(
                    "dependency-task",
                    Box::new(|| {
                        // Condition is always true for this test
                        true
                    }),
                )]
            } else {
                Vec::new()
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
        .add(ConditionalTask {
            id: "dependency-task",
            setup_order: setup_order.clone(),
        })
        .add(ConditionalTask {
            id: "dependent-task",
            setup_order: setup_order.clone(),
        });

    let runtime = supervisor.start().await.unwrap();

    // Give tasks time to setup
    tokio::time::sleep(Duration::from_millis(100)).await;

    runtime.shutdown().await;

    let order = setup_order.lock().unwrap();
    assert_eq!(order.len(), 2);
    // dependency-task should setup before dependent-task
    assert_eq!(order[0], "dependency-task");
    assert_eq!(order[1], "dependent-task");
}

#[tokio::test]
async fn test_conditional_dependencies_disabled() {
    struct EnvBasedTask {
        id: &'static str,
        executed: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for EnvBasedTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn conditional_dependencies(
            &self,
        ) -> Vec<(&'static str, Box<dyn Fn() -> bool + Send + Sync>)> {
            if self.id == "main-task" {
                vec![(
                    "optional-cache",
                    Box::new(|| {
                        // This condition is false, so dependency won't be enforced
                        std::env::var("ENABLE_CACHE").is_ok()
                    }),
                )]
            } else {
                Vec::new()
            }
        }

        async fn run(&self) -> anyhow::Result<()> {
            self.executed.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let executed = Arc::new(AtomicUsize::new(0));

    // Note: ENABLE_CACHE env var is not set, so the conditional dependency won't be active
    // The main-task should run without waiting for optional-cache
    let supervisor = Supervisor::new().add(EnvBasedTask {
        id: "main-task",
        executed: executed.clone(),
    });

    let runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    runtime.shutdown().await;

    assert_eq!(executed.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_active_dependencies_combination() {
    struct MixedDepsTask {
        id: &'static str,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for MixedDepsTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn dependencies(&self) -> &'static [&'static str] {
            if self.id == "task-with-both" {
                &["regular-dep"]
            } else {
                &[]
            }
        }

        fn conditional_dependencies(
            &self,
        ) -> Vec<(&'static str, Box<dyn Fn() -> bool + Send + Sync>)> {
            if self.id == "task-with-both" {
                vec![
                    ("conditional-dep-1", Box::new(|| true)),  // Will be active
                    ("conditional-dep-2", Box::new(|| false)), // Won't be active
                ]
            } else {
                Vec::new()
            }
        }

        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    // Create tasks for all dependencies
    let supervisor = Supervisor::new()
        .add(MixedDepsTask { id: "regular-dep" })
        .add(MixedDepsTask {
            id: "conditional-dep-1",
        })
        .add(MixedDepsTask {
            id: "conditional-dep-2",
        }) // This exists but won't be a dependency
        .add(MixedDepsTask {
            id: "task-with-both",
        });

    let runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    runtime.shutdown().await;

    // Test passes if no errors during startup (meaning dependency resolution worked)
}
