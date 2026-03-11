use foxtive_supervisor::Supervisor;
use foxtive_supervisor::contracts::SupervisedTask;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::{error, info, warn};

pub struct GracefulShutdownTask {
    name: String,
    fail_count: AtomicUsize,
    max_fails: usize,
}

#[async_trait::async_trait]
impl SupervisedTask for GracefulShutdownTask {
    fn id(&self) -> &'static str {
        "graceful-shutdown-task"
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(&self) -> anyhow::Result<()> {
        info!("[{}] Running task", self.name);

        let count = self.fail_count.fetch_add(1, Ordering::SeqCst);
        if count < self.max_fails {
            anyhow::bail!("Simulated failure {}", count);
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        Ok(())
    }

    async fn on_error(&self, _error: &str, _attempt: usize) {
        warn!("[{}] Task failed", self.name);
    }

    async fn on_shutdown(&self) {
        warn!("[{}] Shutting down task", self.name);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let mut supervisor = Supervisor::new()
        .add_many(vec![
            GracefulShutdownTask {
                name: "test_task_1".to_string(),
                fail_count: AtomicUsize::new(0),
                max_fails: 1,
            },
            GracefulShutdownTask {
                name: "test_task_2".to_string(),
                fail_count: AtomicUsize::new(0),
                max_fails: 10000000,
            },
        ])
        .start()
        .await?;

    // Wait for SIGTERM or SIGINT
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Shutting down gracefully...");
            supervisor.shutdown().await;
        }
        result = supervisor.wait_all() => {
            error!("Task died: {:?}", result);
        }
    }

    Ok(())
}
