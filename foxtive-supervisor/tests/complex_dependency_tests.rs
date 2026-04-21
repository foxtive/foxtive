mod common;
use foxtive_supervisor::Supervisor;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[tokio::test]
async fn test_diamond_dependency_graph() {
    // Creates a diamond-shaped dependency graph:
    //        A
    //       / \
    //      B   C
    //       \ /
    //        D

    struct DiamondTask {
        id: &'static str,
        deps: &'static [&'static str],
        setup_order: Arc<std::sync::Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for DiamondTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn dependencies(&self) -> &'static [&'static str] {
            self.deps
        }

        async fn setup(&self) -> anyhow::Result<()> {
            {
                let mut order = self.setup_order.lock().unwrap();
                order.push(self.id.to_string());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(())
        }

        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let setup_order = Arc::new(std::sync::Mutex::new(Vec::new()));

    let supervisor = Supervisor::new()
        .add(DiamondTask {
            id: "A",
            deps: &[], // Root
            setup_order: setup_order.clone(),
        })
        .add(DiamondTask {
            id: "B",
            deps: &["A"], // Depends on A
            setup_order: setup_order.clone(),
        })
        .add(DiamondTask {
            id: "C",
            deps: &["A"], // Depends on A
            setup_order: setup_order.clone(),
        })
        .add(DiamondTask {
            id: "D",
            deps: &["B", "C"], // Depends on both B and C
            setup_order: setup_order.clone(),
        });

    let runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    runtime.shutdown().await;

    let order = setup_order.lock().unwrap();
    assert_eq!(order.len(), 4);

    // A must be first (no dependencies)
    assert_eq!(order[0], "A");

    // B and C can be in any order, but both after A
    assert!(order[1] == "B" || order[1] == "C");
    assert!(order[2] == "B" || order[2] == "C");

    // D must be last (depends on both B and C)
    assert_eq!(order[3], "D");
}

#[tokio::test]
async fn test_deep_linear_chain() {
    // Creates a deep linear chain: A -> B -> C -> D -> E

    struct ChainTask {
        id: &'static str,
        deps: &'static [&'static str],
        execution_order: Arc<std::sync::Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for ChainTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn dependencies(&self) -> &'static [&'static str] {
            self.deps
        }

        async fn setup(&self) -> anyhow::Result<()> {
            {
                let mut order = self.execution_order.lock().unwrap();
                order.push(format!("{}-setup", self.id));
            }
            Ok(())
        }

        async fn run(&self) -> anyhow::Result<()> {
            {
                let mut order = self.execution_order.lock().unwrap();
                order.push(format!("{}-run", self.id));
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok(())
        }
    }

    let execution_order = Arc::new(std::sync::Mutex::new(Vec::new()));

    let supervisor = Supervisor::new()
        .add(ChainTask {
            id: "A",
            deps: &[],
            execution_order: execution_order.clone(),
        })
        .add(ChainTask {
            id: "B",
            deps: &["A"],
            execution_order: execution_order.clone(),
        })
        .add(ChainTask {
            id: "C",
            deps: &["B"],
            execution_order: execution_order.clone(),
        })
        .add(ChainTask {
            id: "D",
            deps: &["C"],
            execution_order: execution_order.clone(),
        })
        .add(ChainTask {
            id: "E",
            deps: &["D"],
            execution_order: execution_order.clone(),
        });

    let runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(300)).await;

    runtime.shutdown().await;

    let order = execution_order.lock().unwrap();

    // Verify setup order respects dependencies
    // A-setup must come before B-setup, etc.
    let a_setup_pos = order.iter().position(|x| x == "A-setup").unwrap();
    let b_setup_pos = order.iter().position(|x| x == "B-setup").unwrap();
    let c_setup_pos = order.iter().position(|x| x == "C-setup").unwrap();
    let d_setup_pos = order.iter().position(|x| x == "D-setup").unwrap();
    let e_setup_pos = order.iter().position(|x| x == "E-setup").unwrap();

    assert!(a_setup_pos < b_setup_pos);
    assert!(b_setup_pos < c_setup_pos);
    assert!(c_setup_pos < d_setup_pos);
    assert!(d_setup_pos < e_setup_pos);

    // All 5 tasks should have executed (setup + run)
    assert_eq!(order.len(), 10);
}

#[tokio::test]
async fn test_multiple_independent_chains() {
    // Creates two independent chains that can run in parallel:
    // Chain 1: A -> B -> C
    // Chain 2: X -> Y -> Z

    struct IndependentTask {
        id: &'static str,
        deps: &'static [&'static str],
        completed: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for IndependentTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn dependencies(&self) -> &'static [&'static str] {
            self.deps
        }

        async fn run(&self) -> anyhow::Result<()> {
            self.completed.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let completed = Arc::new(AtomicUsize::new(0));

    let supervisor = Supervisor::new()
        // Chain 1
        .add(IndependentTask {
            id: "A",
            deps: &[],
            completed: completed.clone(),
        })
        .add(IndependentTask {
            id: "B",
            deps: &["A"],
            completed: completed.clone(),
        })
        .add(IndependentTask {
            id: "C",
            deps: &["B"],
            completed: completed.clone(),
        })
        // Chain 2
        .add(IndependentTask {
            id: "X",
            deps: &[],
            completed: completed.clone(),
        })
        .add(IndependentTask {
            id: "Y",
            deps: &["X"],
            completed: completed.clone(),
        })
        .add(IndependentTask {
            id: "Z",
            deps: &["Y"],
            completed: completed.clone(),
        });

    let runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(300)).await;

    runtime.shutdown().await;

    // All 6 tasks should have completed
    assert_eq!(completed.load(Ordering::SeqCst), 6);
}

#[tokio::test]
async fn test_complex_dag_with_conditional_deps() {
    // Creates a complex DAG with conditional dependencies:
    //     Base
    //    / | \
    //   S1 S2 S3  (S3 depends on Base conditionally)
    //    \\ | /
    //    Aggregator

    struct ComplexTask {
        id: &'static str,
        regular_deps: &'static [&'static str],
        has_conditional_dep: bool,
        execution_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for ComplexTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn dependencies(&self) -> &'static [&'static str] {
            self.regular_deps
        }

        fn conditional_dependencies(
            &self,
        ) -> Vec<(&'static str, Box<dyn Fn() -> bool + Send + Sync>)> {
            if self.has_conditional_dep {
                vec![("Base", Box::new(|| true))]
            } else {
                vec![]
            }
        }

        async fn run(&self) -> anyhow::Result<()> {
            self.execution_count.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(30)).await;
            Ok(())
        }
    }

    let exec_count = Arc::new(AtomicUsize::new(0));

    let supervisor = Supervisor::new()
        .add(ComplexTask {
            id: "Base",
            regular_deps: &[],
            has_conditional_dep: false,
            execution_count: exec_count.clone(),
        })
        .add(ComplexTask {
            id: "S1",
            regular_deps: &["Base"],
            has_conditional_dep: false,
            execution_count: exec_count.clone(),
        })
        .add(ComplexTask {
            id: "S2",
            regular_deps: &["Base"],
            has_conditional_dep: false,
            execution_count: exec_count.clone(),
        })
        .add(ComplexTask {
            id: "S3",
            regular_deps: &[],
            has_conditional_dep: true, // Conditional dep on Base
            execution_count: exec_count.clone(),
        })
        .add(ComplexTask {
            id: "Aggregator",
            regular_deps: &["S1", "S2"],
            has_conditional_dep: false, // No conditional deps here for simplicity
            execution_count: exec_count.clone(),
        });

    let runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(300)).await;

    runtime.shutdown().await;

    // All 5 tasks should have executed
    assert_eq!(exec_count.load(Ordering::SeqCst), 5);
}

#[tokio::test]
async fn test_dependency_with_groups() {
    // Tests that dependencies work correctly with task groups

    struct GroupedDepTask {
        id: &'static str,
        deps: &'static [&'static str],
        group: Option<&'static str>,
        setup_complete: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for GroupedDepTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn dependencies(&self) -> &'static [&'static str] {
            self.deps
        }

        fn group_id(&self) -> Option<&'static str> {
            self.group
        }

        async fn setup(&self) -> anyhow::Result<()> {
            self.setup_complete.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let setup_complete = Arc::new(AtomicUsize::new(0));

    let supervisor = Supervisor::new()
        .add(GroupedDepTask {
            id: "db-service",
            deps: &[],
            group: Some("infrastructure"),
            setup_complete: setup_complete.clone(),
        })
        .add(GroupedDepTask {
            id: "cache-service",
            deps: &[],
            group: Some("infrastructure"),
            setup_complete: setup_complete.clone(),
        })
        .add(GroupedDepTask {
            id: "api-server",
            deps: &["db-service", "cache-service"],
            group: Some("application"),
            setup_complete: setup_complete.clone(),
        });

    let runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify all tasks are in correct groups
    let infra_tasks = runtime.list_group_tasks("infrastructure");
    assert_eq!(infra_tasks.len(), 2);

    let app_tasks = runtime.list_group_tasks("application");
    assert_eq!(app_tasks.len(), 1);

    runtime.shutdown().await;

    // All 3 tasks should have completed setup
    assert_eq!(setup_complete.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_fan_out_fan_in_pattern() {
    // Tests fan-out/fan-in pattern:
    //         Source
    //        / | | \
    //      W1 W2 W3 W4  (workers)
    //        \ | | /
    //       Collector

    struct FanTask {
        id: &'static str,
        deps: &'static [&'static str],
        processed: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for FanTask {
        fn id(&self) -> &'static str {
            self.id
        }

        fn dependencies(&self) -> &'static [&'static str] {
            self.deps
        }

        async fn run(&self) -> anyhow::Result<()> {
            self.processed.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let processed = Arc::new(AtomicUsize::new(0));

    let supervisor = Supervisor::new()
        .add(FanTask {
            id: "Source",
            deps: &[],
            processed: processed.clone(),
        })
        .add(FanTask {
            id: "W1",
            deps: &["Source"],
            processed: processed.clone(),
        })
        .add(FanTask {
            id: "W2",
            deps: &["Source"],
            processed: processed.clone(),
        })
        .add(FanTask {
            id: "W3",
            deps: &["Source"],
            processed: processed.clone(),
        })
        .add(FanTask {
            id: "W4",
            deps: &["Source"],
            processed: processed.clone(),
        })
        .add(FanTask {
            id: "Collector",
            deps: &["W1", "W2", "W3", "W4"],
            processed: processed.clone(),
        });

    let runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(300)).await;

    runtime.shutdown().await;

    // All 6 tasks should have processed
    assert_eq!(processed.load(Ordering::SeqCst), 6);
}
