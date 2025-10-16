mod cron_task;
mod server_task;

use crate::cron_task::CronJobTask;
use crate::server_task::HttpServerTask;
use foxtive_supervisor::Supervisor;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let mut supervisor = Supervisor::new()
        .add(CronJobTask)
        .add(HttpServerTask::create("0.0.0.0:3000"))
        .start()
        .await?;

    // Wait for SIGTERM or SIGINT
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Shutting down gracefully...");
            supervisor.shutdown().await;
        }
        result = supervisor.wait_any() => {
            error!(
                "Critical task '{}' terminated unexpectedly: {:?}",
                result.task_name,
                result.final_status
            );
            // Shutdown remaining tasks
            supervisor.shutdown().await;
            std::process::exit(1);
        }
    }

    Ok(())
}
