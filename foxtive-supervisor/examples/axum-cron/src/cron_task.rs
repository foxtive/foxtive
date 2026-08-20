use foxtive_cron::{Cron, CronResult};
use foxtive_supervisor::contracts::SupervisedTask;
use foxtive_supervisor::error::SupervisorError;
use tracing::{info, warn};

pub struct CronJobTask;

#[async_trait::async_trait]
impl SupervisedTask for CronJobTask {
    fn id(&self) -> &'static str {
        "cron-task"
    }

    fn name(&self) -> String {
        "cron-job-task".to_string()
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &["server-task"]
    }

    async fn run(&self) -> foxtive_supervisor::SupervisorResult<()> {
        let mut cron = Cron::new();

        async fn async_runner() -> CronResult<()> {
            info!("Hello from async fn job at {}", chrono::Utc::now());
            Ok(())
        }

        // Async function
        cron.add_job_fn(
            "impulse",
            "Impulse",
            "*/15 * * * * * *", // every 15 seconds
            async_runner,
        )
        .map_err(SupervisorError::wrap)?;

        cron.run().await;

        Ok(())
    }

    async fn on_shutdown(&self) {
        warn!("Shutting down cron task");
    }
}
