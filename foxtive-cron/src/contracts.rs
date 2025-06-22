use crate::CronResult;

/// A trait representing a schedulable job for the Cron system.
///
/// Any struct or closure that implements this trait can be scheduled
/// and executed according to its provided cron schedule expression.
///
/// ## Required Methods
/// - `run`: The asynchronous execution logic of the job.
/// - `name`: A human-readable name for identification and logging.
/// - `schedule`: A cron expression describing when the job should run.
///
/// ## Optional Method
/// - `description`: An optional human-friendly description of the job's purpose.
#[async_trait::async_trait]
pub trait JobContract: Send + Sync {
    /// The asynchronous logic that should be run when the job is triggered.
    ///
    /// This method is called every time the scheduler reaches the
    /// job's scheduled time.
    ///
    /// # Returns
    /// - `Ok(())` if the job completed successfully.
    /// - `Err(anyhow::Error)` if an error occurred during execution.
    async fn run(&self) -> CronResult<()>;

    /// A unique or descriptive name for the job.
    ///
    /// This name is used in logs and debugging output to identify which job is running.
    fn name(&self) -> String;

    /// A cron expression specifying when the job should be executed.
    ///
    /// The format should be compatible with the `cron` crate, typically in the
    /// form `"*/5 * * * * * *"` for every 5 seconds, or `"0 0 * * * *"` for daily jobs.
    fn schedule(&self) -> String;

    /// A brief optional description of the job.
    ///
    /// This can be used to give context about what the job does.
    /// Defaults to `None`.
    fn description(&self) -> Option<String> {
        None
    }
}
