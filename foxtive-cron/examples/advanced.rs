use async_trait::async_trait;
use chrono::Utc;
use foxtive_cron::contracts::{
    JobContract, JobEvent, JobEventListener, MetricsExporter, MisfirePolicy, RetryPolicy,
    ValidatedSchedule,
};
use foxtive_cron::{Cron, CronError, CronResult};
use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

/// A custom job struct implementing the `JobContract` trait.
/// This allows for full control over job behavior, including state,
/// retry policies, and lifecycle hooks.
struct DatabaseBackupJob {
    id: String,
    schedule: ValidatedSchedule,
}

#[async_trait]
impl JobContract for DatabaseBackupJob {
    async fn run(&self) -> CronResult<()> {
        println!("[{}] Starting database backup...", self.id);
        // Simulate some work
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Randomly fail to demonstrate retry and error handling
        if Utc::now().timestamp() % 3 == 0 {
            return Err(CronError::ExecutionError(anyhow::anyhow!(
                "Database connection lost"
            )));
        }

        println!("[{}] Backup completed successfully.", self.id);
        Ok(())
    }

    fn id(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.id)
    }

    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("Database Backup")
    }

    fn schedule(&self) -> &ValidatedSchedule {
        &self.schedule
    }

    fn timeout(&self) -> Option<Duration> {
        // Enforce a 5-second timeout on this job
        Some(Duration::from_secs(5))
    }

    fn retry_policy(&self) -> RetryPolicy {
        // Retry up to 3 times with exponential backoff
        RetryPolicy::Exponential {
            max_retries: 3,
            initial_interval: Duration::from_secs(1),
            max_interval: Duration::from_secs(10),
        }
    }

    fn misfire_policy(&self) -> MisfirePolicy {
        // If we miss a run, fire once as soon as possible
        MisfirePolicy::FireOnce
    }

    async fn on_start(&self) {
        println!("[{}] Lifecycle: Job starting", self.id);
    }

    async fn on_complete(&self) {
        println!("[{}] Lifecycle: Job finished successfully", self.id);
    }

    async fn on_error(&self, error: &CronError) {
        println!(
            "[{}] Lifecycle: Job failed with error: {:?}",
            self.id, error
        );
    }
}

/// A simple event listener that logs all job events.
struct LoggerListener;

#[async_trait]
impl JobEventListener for LoggerListener {
    async fn on_event(&self, event: JobEvent) {
        match event {
            JobEvent::Started { id, name } => println!("EVENT: Job '{}' ({}) started", name, id),
            JobEvent::Completed { id, duration, .. } => {
                println!("EVENT: Job '{}' completed in {:?}", id, duration)
            }
            JobEvent::Failed { id, error, .. } => println!("EVENT: Job '{}' failed: {}", id, error),
            JobEvent::Retrying {
                id, attempt, delay, ..
            } => {
                println!(
                    "EVENT: Job '{}' retrying (attempt {}) in {:?}",
                    id, attempt, delay
                )
            }
            JobEvent::Misfired {
                id, scheduled_time, ..
            } => {
                println!(
                    "EVENT: Job '{}' misfired. Was scheduled for {}",
                    id, scheduled_time
                )
            }
        }
    }
}

/// A custom metrics exporter (mock).
struct PrintMetricsExporter;

impl MetricsExporter for PrintMetricsExporter {
    fn record_start(&self, _id: &str, _name: &str) {}
    fn record_completion(&self, id: &str, _name: &str, duration: Duration) {
        println!("METRIC: Job '{}' took {:?}", id, duration);
    }
    fn record_failure(&self, id: &str, _name: &str) {
        println!("METRIC: Job '{}' failed", id);
    }
    fn record_retry(&self, id: &str, _name: &str) {
        println!("METRIC: Job '{}' retrying", id);
    }
    fn record_misfire(&self, id: &str, _name: &str) {
        println!("METRIC: Job '{}' misfired", id);
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing for logs
    tracing_subscriber::fmt::init();

    // Use the builder to configure the scheduler
    let mut cron = Cron::builder()
        .with_global_concurrency_limit(5)
        .with_listener(Arc::new(LoggerListener))
        .with_metrics_exporter(Arc::new(PrintMetricsExporter))
        .build();

    // Add our complex job
    let backup_job = DatabaseBackupJob {
        id: "daily-backup".to_string(),
        schedule: ValidatedSchedule::parse("*/10 * * * * * *").unwrap(), // Every 10 seconds for demo
    };
    cron.add_job(backup_job).expect("Failed to add job");

    // Add a simple job using a closure
    cron.add_job_fn("heartbeat", "Heartbeat", "*/5 * * * * * *", || async {
        println!("Heartbeat: System is healthy at {}", Utc::now());
        Ok(())
    })
    .unwrap();

    println!("Scheduler starting. Press Ctrl+C to stop.");

    // Handle graceful shutdown
    let mut cron_handle = cron;
    tokio::select! {
        _ = cron_handle.run() => {},
        _ = tokio::signal::ctrl_c() => {
            println!("\nShutdown signal received.");
            cron_handle.shutdown().await;
        }
    }
}
