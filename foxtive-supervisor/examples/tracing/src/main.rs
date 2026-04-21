use foxtive_supervisor::contracts::SupervisedTask;
use foxtive_supervisor::Supervisor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tracing::{info, info_span, Instrument};

pub struct TracedTask {
    id: &'static str,
    fail_count: AtomicUsize,
    max_fails: usize,
}

#[async_trait::async_trait]
impl SupervisedTask for TracedTask {
    fn id(&self) -> &'static str {
        self.id
    }

    async fn run(&self) -> anyhow::Result<()> {
        let span = info_span!("traced_task_run", task_id = self.id);
        async move {
            info!("Starting business logic in task {}", self.id);

            tokio::time::sleep(Duration::from_millis(300)).await;

            let count = self.fail_count.fetch_add(1, Ordering::SeqCst);
            if count < self.max_fails {
                info!("Simulating failure {}/{}", count + 1, self.max_fails);
                anyhow::bail!("Simulated business logic failure");
            }

            info!("Business logic completed successfully");
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(())
        }
        .instrument(span)
        .await
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Use a standard fmt subscriber to emit internal instrumentation
    tracing_subscriber::fmt::init();

    info!("Starting supervisor with tracing instrumentation...");

    let supervisor = Supervisor::new()
        .add(TracedTask {
            id: "database-worker",
            fail_count: AtomicUsize::new(0),
            max_fails: 2,
        })
        .add(TracedTask {
            id: "api-fetcher",
            fail_count: AtomicUsize::new(0),
            max_fails: 0,
        });

    info!("Waiting for tasks to complete...");
    let results = supervisor.start_and_wait_all().await?;

    for result in results {
        info!(
            "Task '{}' (id: {}) finished with {:?} after {} attempts",
            result.task_name, result.task_id, result.final_status, result.total_attempts
        );
    }

    Ok(())
}
