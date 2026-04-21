mod common;
use common::*;
use foxtive_supervisor::Supervisor;
use foxtive_supervisor::enums::SupervisionStatus;

#[tokio::test]
async fn test_max_attempts_exceeded() {
    let task = MockTask::new("max_attempts_test")
        .with_failures(10)
        .with_policy(foxtive_supervisor::enums::RestartPolicy::MaxAttempts(3));

    let result = Supervisor::new()
        .add(task)
        .start_and_wait_any()
        .await
        .unwrap();

    assert_eq!(result.final_status, SupervisionStatus::MaxAttemptsReached);
    assert_eq!(result.total_attempts, 3);
}

#[tokio::test]
async fn test_restart_prevented_by_should_restart() {
    struct ConditionalTask {
        id: &'static str,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for ConditionalTask {
        fn id(&self) -> &'static str {
            self.id
        }
        async fn run(&self) -> anyhow::Result<()> {
            anyhow::bail!("Always fails")
        }
        async fn should_restart(&self, _attempt: usize, _error: &str) -> bool {
            false
        }
    }

    let result = Supervisor::new()
        .add(ConditionalTask { id: "no_restart" })
        .start_and_wait_any()
        .await
        .unwrap();

    assert_eq!(result.final_status, SupervisionStatus::RestartPrevented);
    assert_eq!(result.total_attempts, 1);
}

#[tokio::test]
async fn test_dependency_failure_prevents_startup() {
    struct FailingSetupTask {
        id: &'static str,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for FailingSetupTask {
        fn id(&self) -> &'static str {
            self.id
        }
        async fn setup(&self) -> anyhow::Result<()> {
            anyhow::bail!("Setup failed")
        }
        async fn run(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct DependentTask {
        id: &'static str,
        dep_id: &'static str,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for DependentTask {
        fn id(&self) -> &'static str {
            self.id
        }
        fn dependencies(&self) -> &'static [&'static str] {
            Box::leak(vec![self.dep_id].into_boxed_slice())
        }
        async fn run(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    let supervisor = Supervisor::new()
        .add(FailingSetupTask {
            id: "failing_setup",
        })
        .add(DependentTask {
            id: "dependent",
            dep_id: "failing_setup",
        });

    let mut runtime = supervisor.start().await.unwrap();
    let result = runtime.wait_any().await;

    // Either failing_setup fails setup, or dependent fails its dependency.
    // wait_any returns the first one that terminates.
    assert!(
        result.final_status == SupervisionStatus::SetupFailed
            || result.final_status == SupervisionStatus::DependencyFailed
    );
}
