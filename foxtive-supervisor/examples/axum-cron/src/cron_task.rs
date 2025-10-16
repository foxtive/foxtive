use foxtive_cron::{Cron, CronResult};
use foxtive_supervisor::contracts::SupervisedTask;
use tracing::{info, warn};

pub struct CronJobTask;

#[async_trait::async_trait]
impl SupervisedTask for CronJobTask {
    fn name(&self) -> String {
        "cron-job-task".to_string()
    }

    async fn run(&self) -> anyhow::Result<()> {
        let mut cron = Cron::new();

        async fn async_runner() -> CronResult<()> {
            info!("Hello from async fn job at {}", chrono::Utc::now());
            Ok(())
        }

        // Async function
        cron.add_job_fn(
            "Impulse",
            "*/15 * * * * * *", // every 15 seconds
            async_runner,
        )?;

        cron.run().await;

        Ok(())
    }

    async fn on_shutdown(&self) {
        warn!("Shutting down cron task");
    }
}
