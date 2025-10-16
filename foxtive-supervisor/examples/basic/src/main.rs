use foxtive_supervisor::contracts::SupervisedTask;
use foxtive_supervisor::Supervisor;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::info;

pub struct BasicTask {
    name: String,
    fail_count: AtomicUsize,
    max_fails: usize,
}

#[async_trait::async_trait]
impl SupervisedTask for BasicTask {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(&self) -> anyhow::Result<()> {
        info!("Running task {}", self.name);

        let count = self.fail_count.fetch_add(1, Ordering::SeqCst);
        if count < self.max_fails {
            anyhow::bail!("Simulated failure {}", count);
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        Ok(())
    }

    async fn on_error(&self, _error: &str, _attempt: usize) {
        println!("Task failed");
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let results = Supervisor::new()
        .add(BasicTask {
            name: "test_task_1".to_string(),
            fail_count: AtomicUsize::new(0),
            max_fails: 2,
        })
        .start_and_wait_all()
        .await?;

    for result in results {
        info!(
            "Task '{}' finished with {:?} after {} attempts",
            result.task_name, result.final_status, result.total_attempts
        );
    }

    Ok(())
}
