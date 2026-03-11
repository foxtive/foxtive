use crate::CronResult;
use crate::contracts::{JobContract, ValidatedSchedule};
use async_trait::async_trait;
use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type RunnableFunc =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = CronResult<()>> + Send>> + Send + Sync>;

/// A lightweight, closure-based implementation of [`JobContract`].
///
/// `FnJob` allows scheduling arbitrary `async fn`s or closures without the need to define
/// a new struct that implements [`JobContract`]. It also supports scheduling
/// **blocking** (synchronous) functions using `tokio::task::spawn_blocking`.
///
/// The cron schedule is validated eagerly at construction time via
/// [`ValidatedSchedule::parse`], so registration fails fast on a bad expression.
///
/// ## Example: Async Job
/// ```rust
/// use foxtive_cron::Cron;
///
/// let mut cron = Cron::new();
/// let _ = cron.add_job_fn("heartbeat", "Heartbeat", "*/10 * * * * * *", || async {
///     println!("Heartbeat ping");
///     Ok(())
/// });
/// ```
///
/// ## Example: Blocking Job
/// ```rust
/// use foxtive_cron::Cron;
///
/// let mut cron = Cron::new();
/// let _ = cron.add_blocking_job_fn("backup", "Backup", "0 0 * * * * *", || {
///     std::fs::write("/backup", "snapshot").unwrap();
///     Ok(())
/// });
/// ```
pub struct FnJob {
    id: String,
    name: String,
    schedule: ValidatedSchedule,
    func: RunnableFunc,
}

#[async_trait]
impl JobContract for FnJob {
    async fn run(&self) -> CronResult<()> {
        (self.func)().await
    }

    fn id(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.id)
    }

    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.name)
    }

    fn schedule(&self) -> &ValidatedSchedule {
        &self.schedule
    }
}

impl FnJob {
    /// Creates a new `FnJob` from an async closure or function.
    ///
    /// # Parameters
    /// - `id`: A stable unique identifier for this job (used for deduplication/cancellation).
    /// - `name`: A human-readable label used in logs.
    /// - `schedule_expr`: A cron expression string defining when the job should run.
    /// - `func`: An async closure or function to run at the scheduled time.
    ///
    /// # Errors
    /// Returns an error if `schedule_expr` is not a valid cron expression.
    ///
    /// # Example
    /// ```rust
    /// use foxtive_cron::FnJob;
    ///
    /// let job = FnJob::new("test-job", "Test Job", "*/5 * * * * * *", || async {
    ///     println!("Running test job");
    ///     Ok(())
    /// }).unwrap();
    /// ```
    pub fn new<F, Fut>(
        id: impl Into<String>,
        name: impl Into<String>,
        schedule_expr: &str,
        func: F,
    ) -> CronResult<Self>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = CronResult<()>> + Send + 'static,
    {
        Ok(Self {
            id: id.into(),
            name: name.into(),
            schedule: ValidatedSchedule::parse(schedule_expr)?,
            func: Arc::new(move || Box::pin(func())),
        })
    }

    /// Creates a new `FnJob` from a **blocking** function.
    ///
    /// The function will be run inside `tokio::task::spawn_blocking` to avoid
    /// blocking the async runtime.
    ///
    /// # Parameters
    /// - `id`: A stable unique identifier for this job.
    /// - `name`: A human-readable label used in logs.
    /// - `schedule_expr`: A cron expression string.
    /// - `func`: A blocking function that returns `CronResult<()>`.
    ///
    /// # Errors
    /// Returns an error if `schedule_expr` is not a valid cron expression.
    ///
    /// # Example
    /// ```rust
    /// use foxtive_cron::FnJob;
    ///
    /// let job = FnJob::new_blocking("heavy-job", "Heavy Computation", "*/10 * * * * * *", || {
    ///     std::thread::sleep(std::time::Duration::from_secs(2));
    ///     Ok(())
    /// }).unwrap();
    /// ```
    pub fn new_blocking<F>(
        id: impl Into<String>,
        name: impl Into<String>,
        schedule_expr: &str,
        func: F,
    ) -> CronResult<Self>
    where
        F: Fn() -> CronResult<()> + Send + Sync + 'static + Clone,
    {
        Ok(Self {
            id: id.into(),
            name: name.into(),
            schedule: ValidatedSchedule::parse(schedule_expr)?,
            func: Arc::new(move || {
                let f = func.clone();
                Box::pin(async move { tokio::task::spawn_blocking(f).await? })
            }),
        })
    }
}
