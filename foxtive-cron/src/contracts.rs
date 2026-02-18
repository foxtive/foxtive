use crate::CronResult;
use std::borrow::Cow;
use std::str::FromStr;

/// A validated cron schedule, parsed at construction time to prevent
/// runtime errors from malformed expressions.
///
/// Wraps [`cron::Schedule`] and is the required return type of [`JobContract::schedule`].
/// By accepting only `ValidatedSchedule` values, the scheduler guarantees that
/// no job can be registered with an invalid cron expression.
#[derive(Debug)]
pub struct ValidatedSchedule(pub(crate) cron::Schedule);

impl ValidatedSchedule {
    /// Parse and validate a cron expression.
    ///
    /// # Errors
    /// Returns an error if the expression is not a valid cron format.
    ///
    /// # Example
    /// ```rust
    /// use foxtive_cron::contracts::ValidatedSchedule;
    ///
    /// let schedule = ValidatedSchedule::parse("*/5 * * * * * *").unwrap();
    /// ```
    pub fn parse(expr: &str) -> CronResult<Self> {
        let schedule = cron::Schedule::from_str(expr)
            .map_err(|e| anyhow::anyhow!("Invalid cron expression '{}': {}", expr, e))?;
        Ok(Self(schedule))
    }
}

/// A trait representing a schedulable job for the Cron system.
///
/// Any struct that implements this trait can be scheduled and executed
/// according to its provided cron schedule.
///
/// ## Required Methods
/// - `run`: The asynchronous execution logic of the job.
/// - `id`: A stable unique identifier used for cancellation and deduplication.
/// - `name`: A human-readable name for identification and logging.
/// - `schedule`: A pre-validated [`ValidatedSchedule`] describing when the job runs.
///
/// ## Optional Methods
/// - `description`: An optional human-friendly description of the job's purpose.
/// - `on_start`: Lifecycle hook called just before `run`.
/// - `on_complete`: Lifecycle hook called after a successful `run`.
/// - `on_error`: Lifecycle hook called when `run` returns an error.
#[async_trait::async_trait]
pub trait JobContract: Send + Sync {
    /// The asynchronous logic to run when the job is triggered.
    ///
    /// Called every time the scheduler reaches the job's scheduled time.
    ///
    /// # Returns
    /// - `Ok(())` if the job completed successfully.
    /// - `Err(anyhow::Error)` if an error occurred during execution.
    async fn run(&self) -> CronResult<()>;

    /// A stable unique identifier for this job.
    ///
    /// Used for cancellation, deduplication, and internal tracking.
    /// Should remain consistent across restarts (e.g. a static string or UUID).
    fn id(&self) -> Cow<'_, str>;

    /// A human-readable name for the job.
    ///
    /// Used in logs and debug output. Does not need to be globally unique.
    fn name(&self) -> Cow<'_, str>;

    /// The validated cron schedule describing when this job should run.
    ///
    /// Return a [`ValidatedSchedule`] constructed via [`ValidatedSchedule::parse`],
    /// which ensures the expression is valid before the job is ever registered.
    fn schedule(&self) -> &ValidatedSchedule;

    /// A brief optional description of what this job does.
    ///
    /// Defaults to `None`.
    fn description(&self) -> Option<Cow<'_, str>> {
        None
    }

    /// Called just before [`run`](Self::run) is invoked.
    ///
    /// Useful for metrics, logging, or pre-flight checks.
    /// Defaults to a no-op.
    async fn on_start(&self) {}

    /// Called after [`run`](Self::run) completes successfully.
    ///
    /// Useful for metrics or post-processing.
    /// Defaults to a no-op.
    async fn on_complete(&self) {}

    /// Called if [`run`](Self::run) returns an `Err`.
    ///
    /// Useful for alerting, retry logic, or error reporting.
    /// Defaults to a no-op.
    async fn on_error(&self, _error: &anyhow::Error) {}
}