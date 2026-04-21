use crate::{CronError, CronResult};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Policies for handling missed job executions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MisfirePolicy {
    /// Skip all missed runs and wait for the next scheduled occurrence.
    #[default]
    Skip,
    /// Execute a single missed run as soon as possible, then resume regular schedule.
    FireOnce,
    /// Execute all missed runs as soon as possible, one by one.
    FireAll,
}

/// Policies for retrying failed job runs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum RetryPolicy {
    /// No retries.
    #[default]
    None,
    /// Retry a fixed number of times with a specific interval.
    Fixed {
        max_retries: usize,
        interval: Duration,
    },
    /// Exponential backoff retry strategy.
    Exponential {
        max_retries: usize,
        initial_interval: Duration,
        max_interval: Duration,
    },
}

/// Events emitted by the scheduler during the job lifecycle.
#[derive(Debug, Clone)]
pub enum JobEvent {
    /// Emitted when a job is about to start.
    Started { id: String, name: String },
    /// Emitted when a job completes successfully.
    Completed {
        id: String,
        name: String,
        duration: Duration,
    },
    /// Emitted when a job fails.
    Failed {
        id: String,
        name: String,
        error: String,
    },
    /// Emitted when a job is scheduled for retry.
    Retrying {
        id: String,
        name: String,
        attempt: usize,
        delay: Duration,
    },
    /// Emitted when a scheduled job misfires.
    Misfired {
        id: String,
        name: String,
        scheduled_time: DateTime<Utc>,
    },
}

/// Trait for listening to scheduler events.
#[async_trait::async_trait]
pub trait JobEventListener: Send + Sync {
    /// Called when an event occurs.
    async fn on_event(&self, event: JobEvent);
}

/// Trait for exporting metrics.
pub trait MetricsExporter: Send + Sync {
    /// Record a job start.
    fn record_start(&self, id: &str, name: &str);
    /// Record a job completion.
    fn record_completion(&self, id: &str, name: &str, duration: Duration);
    /// Record a job failure.
    fn record_failure(&self, id: &str, name: &str);
    /// Record a job retry.
    fn record_retry(&self, id: &str, name: &str);
    /// Record a job misfire.
    fn record_misfire(&self, id: &str, name: &str);
}

/// Information about a job's execution state for persistence.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JobState {
    pub last_run: Option<DateTime<Utc>>,
    pub last_success: Option<DateTime<Utc>>,
    pub last_failure: Option<DateTime<Utc>>,
    pub consecutive_failures: usize,
}

/// Trait for persisting job definitions and states.
#[async_trait::async_trait]
pub trait JobStore: Send + Sync {
    /// Save or update a job's state.
    async fn save_state(&self, id: &str, state: &JobState) -> CronResult<()>;
    /// Retrieve a job's state.
    async fn get_state(&self, id: &str) -> CronResult<Option<JobState>>;
}

/// A simple in-memory implementation of [`JobStore`].
#[derive(Default)]
pub struct InMemoryJobStore {
    states: Arc<RwLock<HashMap<String, JobState>>>,
}

impl InMemoryJobStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl JobStore for InMemoryJobStore {
    async fn save_state(&self, id: &str, state: &JobState) -> CronResult<()> {
        let mut states = self.states.write().await;
        states.insert(id.to_string(), state.clone());
        Ok(())
    }

    async fn get_state(&self, id: &str) -> CronResult<Option<JobState>> {
        let states = self.states.read().await;
        Ok(states.get(id).cloned())
    }
}

/// A validated cron schedule, parsed at construction time to prevent
/// runtime errors from malformed expressions.
///
/// Wraps [`cron::Schedule`] and is the required return type of [`JobContract::schedule`].
/// By accepting only `ValidatedSchedule` values, the scheduler guarantees that
/// no job can be registered with an invalid cron expression.
#[derive(Debug, Clone)]
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
            .map_err(|e| CronError::InvalidSchedule(format!("{}: {}", expr, e)))?;
        Ok(Self(schedule))
    }

    /// Returns the next occurrence after the given time, in the specified timezone.
    pub fn next_after(&self, after: &DateTime<Utc>, tz: Tz) -> Option<DateTime<Utc>> {
        let local_after = after.with_timezone(&tz);
        self.0
            .after(&local_after)
            .next()
            .map(|dt| dt.with_timezone(&Utc))
    }
}

/// A trait that defines a schedule for a job.
pub trait Schedule: Send + Sync {
    /// Returns the next scheduled time after the given timestamp.
    fn next_after(&self, after: &DateTime<Utc>, tz: Tz) -> Option<DateTime<Utc>>;
}

impl Schedule for ValidatedSchedule {
    fn next_after(&self, after: &DateTime<Utc>, tz: Tz) -> Option<DateTime<Utc>> {
        self.next_after(after, tz)
    }
}

/// A type of job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobType {
    /// A job that runs according to a cron schedule.
    Recurring,
    /// A job that runs once at a specific time.
    Once,
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
/// - `schedule`: A description of when the job runs.
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

    /// The schedule describing when this job should run.
    fn schedule(&self) -> &dyn Schedule;

    /// The type of job.
    fn job_type(&self) -> JobType {
        JobType::Recurring
    }

    /// The time at which the job should run if it is a one-time job.
    ///
    /// If `job_type()` returns `JobType::Once`, this must return `Some`.
    fn run_at(&self) -> Option<DateTime<Utc>> {
        None
    }

    /// The time after which the job should start running.
    fn start_after(&self) -> Option<DateTime<Utc>> {
        None
    }

    /// The timezone in which the cron schedule should be evaluated.
    ///
    /// Defaults to `UTC`.
    fn timezone(&self) -> Tz {
        chrono_tz::UTC
    }

    /// A brief optional description of what this job does.
    ///
    /// Defaults to `None`.
    fn description(&self) -> Option<Cow<'_, str>> {
        None
    }

    /// The maximum duration a single job run should take.
    ///
    /// If `None`, the job has no timeout.
    fn timeout(&self) -> Option<Duration> {
        None
    }

    /// The priority of the job. Higher values represent higher priority.
    ///
    /// When multiple jobs are scheduled at the same time, higher priority
    /// jobs will be triggered first.
    fn priority(&self) -> i32 {
        0
    }

    /// The maximum number of concurrent executions for this job.
    ///
    /// If `None`, there's no per-job concurrency limit.
    fn concurrency_limit(&self) -> Option<usize> {
        None
    }

    /// Defines how the scheduler behaves if a scheduled execution is missed.
    fn misfire_policy(&self) -> MisfirePolicy {
        MisfirePolicy::default()
    }

    /// Defines how the scheduler behaves if an execution fails.
    fn retry_policy(&self) -> RetryPolicy {
        RetryPolicy::default()
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
    async fn on_error(&self, _error: &CronError) {}
}
