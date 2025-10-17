use foxtive_supervisor::Supervisor;
use foxtive_supervisor::contracts::SupervisedTask;
use foxtive_supervisor::enums::BackoffStrategy;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tracing::info;

pub struct PanicCatcherTask {
    name: String,
    fail_count: AtomicUsize,
    max_fails: usize,
}

#[async_trait::async_trait]
impl SupervisedTask for PanicCatcherTask {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn backoff_strategy(&self) -> BackoffStrategy {
        BackoffStrategy::Fixed(Duration::from_secs(1))
    }

    async fn run(&self) -> anyhow::Result<()> {
        info!("Running task {}", self.name);

        let count = self.fail_count.fetch_add(1, Ordering::SeqCst);
        if count < self.max_fails {
            panic!("Simulated failure {}", count);
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        Ok(())
    }

    async fn on_panic(&self, _panic_info: &str, _attempt: usize) {
        info!("Task {} Panicked", self.name);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let results = Supervisor::new()
        .add(PanicCatcherTask {
            name: "test_task_1".to_string(),
            fail_count: AtomicUsize::new(0),
            max_fails: 3,
        })
        .add(PanicCatcherTask {
            name: "test_task_2".to_string(),
            fail_count: AtomicUsize::new(0),
            max_fails: 6,
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
