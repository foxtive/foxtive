use crate::contracts::JobContract;
use crate::CronResult;
use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type RunnableFunc = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = CronResult<()>> + Send>> + Send + Sync>;

/// A lightweight, closure-based implementation of a `JobContract`.
///
/// `FnJob` allows scheduling arbitrary `async fn`s or closures without the need to define
/// a new struct that implements the `JobContract` trait. It also supports scheduling
/// **blocking** (synchronous) functions using `tokio::task::spawn_blocking`.
///
/// This is especially useful for quick, inline tasks or wrapping existing functions.
///
/// ## Example: Async Job
/// ```rust
/// use foxtive_cron::Cron;
///
/// let mut cron = Cron::new();
/// let _ = cron.add_job_fn("Heartbeat", "*/10 * * * * * *", || async {
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
/// let _ = cron.add_blocking_job_fn("Backup", "0 0 * * * * *", || {
///     std::fs::write("/backup", "snapshot").unwrap();
///     Ok(())
/// });
/// ```
pub struct FnJob {
    name: String,
    schedule_expr: String,
    /// A boxed, shared async function/closure that returns a `CronResult<()>`.
    func: RunnableFunc,
}

#[async_trait]
impl JobContract for FnJob {
    /// Executes the wrapped async function.
    async fn run(&self) -> CronResult<()> {
        (self.func)().await
    }

    /// Returns the name of the job.
    fn name(&self) -> String {
        self.name.clone()
    }

    /// Returns the cron schedule expression as a string.
    fn schedule(&self) -> String {
        self.schedule_expr.clone()
    }
}

impl FnJob {
    /// Creates a new `FnJob` from an async closure or function.
    ///
    /// # Parameters
    /// - `name`: A human-readable identifier for the job.
    /// - `schedule_expr`: A cron expression string defining when the job should run.
    /// - `func`: An async closure or function to run at the scheduled time.
    ///
    /// # Example
    /// ```rust
    /// use foxtive_cron::FnJob;
    ///
    /// FnJob::new("Test Job", "*/5 * * * * * *", || async {
    ///     println!("Running test job");
    ///     Ok(())
    /// });
    /// ```
    pub fn new<F, Fut>(name: impl Into<String>, schedule_expr: impl Into<String>, func: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = CronResult<()>> + Send + 'static,
    {
        Self {
            name: name.into(),
            schedule_expr: schedule_expr.into(),
            func: Arc::new(move || Box::pin(func())),
        }
    }

    /// Creates a new `FnJob` from a **blocking** function.
    ///
    /// The function will be run inside a `tokio::spawn_blocking` task to avoid blocking
    /// the async runtime.
    ///
    /// # Parameters
    /// - `name`: Job name for identification.
    /// - `schedule_expr`: Cron expression string.
    /// - `func`: A blocking function that returns `CronResult<()>`.
    ///
    /// # Example
    /// ```rust
    /// use foxtive_cron::FnJob;
    ///
    /// FnJob::new_blocking("Heavy Computation", "*/10 * * * * * *", || {
    ///     std::thread::sleep(std::time::Duration::from_secs(2));
    ///     Ok(())
    /// });
    /// ```
    pub fn new_blocking<F>(
        name: impl Into<String>,
        schedule_expr: impl Into<String>,
        func: F,
    ) -> Self
    where
        F: Fn() -> CronResult<()> + Send + Sync + 'static + Clone,
    {
        Self {
            name: name.into(),
            schedule_expr: schedule_expr.into(),
            func: Arc::new(move || {
                let f = func.clone();
                Box::pin(async move {
                    tokio::task::spawn_blocking(f).await?
                })
            }),
        }
    }
}
