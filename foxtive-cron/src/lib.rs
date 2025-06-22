use crate::contracts::JobContract;
use crate::job::JobItem;
use chrono::{DateTime, Utc};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;
use tokio::time::{Instant, sleep_until};

pub mod contracts;
mod fn_job;
mod job;

pub use fn_job::FnJob;

/// A type alias for results returned by cron jobs, using `anyhow::Result`.
pub type CronResult<T> = anyhow::Result<T>;

/// Represents a job scheduled to run at a specific time.
///
/// This is used internally in a min-heap (`BinaryHeap`) to efficiently
/// manage upcoming scheduled job executions.
#[derive(Clone)]
struct ScheduledJob {
    /// The next time this job is scheduled to run.
    next_run: DateTime<Utc>,
    /// The job to execute.
    job: JobItem,
}

// BinaryHeap needs Ord trait to sort ScheduledJob entries.
impl Eq for ScheduledJob {}

impl PartialEq for ScheduledJob {
    fn eq(&self, other: &Self) -> bool {
        self.next_run == other.next_run
    }
}

impl Ord for ScheduledJob {
    /// Defines the reverse ordering so that the job with the **earliest `next_run`**
    /// appears at the top of the heap.
    fn cmp(&self, other: &Self) -> Ordering {
        other.next_run.cmp(&self.next_run)
    }
}

impl PartialOrd for ScheduledJob {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Default for Cron {
    fn default() -> Self {
        Self::new()
    }
}

/// An asynchronous job scheduler that runs registered jobs based on cron expressions.
///
/// `Cron` supports:
/// - Adding fully custom jobs via the [`JobContract`] trait
/// - Registering async closures using [`add_job_fn`]
/// - Registering blocking closures using [`add_blocking_job_fn`]
///
/// Jobs are executed asynchronously and are automatically rescheduled after each execution
/// according to their cron schedule.
///
/// ### Example
/// ```no_run
/// use foxtive_cron::Cron;
///
/// let mut cron = Cron::new();
///
/// let _ = cron.add_job_fn("Ping", "*/5 * * * * * *", || async {
///     println!("Ping at {}", chrono::Utc::now());
///     Ok(())
/// });
///
/// tokio::spawn(async move { cron.run().await });
/// ```
pub struct Cron {
    queue: BinaryHeap<ScheduledJob>,
}

impl Cron {
    /// Creates a new empty Cron scheduler.
    pub fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
        }
    }

    /// Adds a custom job that implements the [`JobContract`] trait.
    ///
    /// This is the most flexible way to schedule complex job types.
    ///
    /// # Parameters
    /// - `job`: An `Arc<dyn JobContract>` instance representing the job to run.
    ///
    /// # Returns
    /// `Ok(())` if the job was added successfully, otherwise an error.
    pub fn add_job(&mut self, job: Arc<dyn JobContract>) -> CronResult<()> {
        let job = JobItem::new(job)?;
        if let Some(next_run) = job.next_run_time() {
            self.queue.push(ScheduledJob { next_run, job });
        }

        Ok(())
    }

    /// Adds a job from an asynchronous closure or `async fn`.
    ///
    /// The closure will be wrapped in an internal job adapter that implements `JobContract`.
    ///
    /// # Example
    /// ```rust
    /// use foxtive_cron::Cron;
    ///
    /// let mut cron = Cron::new();
    ///
    /// let _ = cron.add_job_fn("Heartbeat", "*/10 * * * * * *", || async {
    ///     println!("Heartbeat ping");
    ///     Ok(())
    /// });
    /// ```
    pub fn add_job_fn<F, Fut>(
        &mut self,
        name: impl Into<String>,
        schedule_expr: impl Into<String>,
        func: F,
    ) -> CronResult<()>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = CronResult<()>> + Send + 'static,
    {
        let job = Arc::new(FnJob::new(name, schedule_expr, func));
        self.add_job(job)
    }

    /// Adds a job from a **blocking** closure or function.
    ///
    /// This is useful for CPU-heavy or synchronous operations such as:
    /// file I/O, backups, compression, etc.
    ///
    /// The function is executed in a safe blocking thread using `tokio::spawn_blocking`.
    ///
    /// # Example
    /// ```rust
    /// use foxtive_cron::Cron;
    ///
    /// let mut cron = Cron::new();
    ///
    /// let _ = cron.add_blocking_job_fn("Compress", "0 0 * * * * *", || {
    ///     std::thread::sleep(std::time::Duration::from_secs(1));
    ///     Ok(())
    /// });
    /// ```
    pub fn add_blocking_job_fn<F>(
        &mut self,
        name: impl Into<String>,
        schedule_expr: impl Into<String>,
        func: F,
    ) -> CronResult<()>
    where
        F: Fn() -> CronResult<()> + Send + Sync + 'static + Clone,
    {
        let job = Arc::new(FnJob::new_blocking(name, schedule_expr, func));
        self.add_job(job)
    }

    /// Starts the scheduler loop.
    ///
    /// This method will continuously wait for the next scheduled job,
    /// execute it asynchronously, and re-schedule it for its next run.
    ///
    /// It should be spawned using `tokio::spawn` or awaited directly.
    ///
    /// # Example
    /// ```no_run
    /// use foxtive_cron::Cron;
    ///
    /// let mut cron = Cron::new();
    ///
    /// tokio::spawn(async move {
    ///     cron.run().await;
    /// });
    /// ```
    pub async fn run(&mut self) {
        while let Some(ScheduledJob { next_run, job }) = self.queue.pop() {
            let now = Utc::now();

            if next_run > now {
                let delay = (next_run - now).to_std().unwrap_or_default();
                sleep_until(Instant::now() + delay).await;
            }

            let name = job.name().clone();
            let job_clone = job.clone();

            // Run a job in a separate task
            tokio::spawn(async move {
                log::info!("[{}] Running job", name);
                if let Err(err) = job_clone.run().await {
                    log::error!("[{}] Job failed: {:?}", name, err);
                } else {
                    log::info!("[{}] Job completed", name);
                }
            });

            // Re-schedule the job
            if let Some(next_run) = job.next_run_time() {
                self.queue.push(ScheduledJob { next_run, job });
            }
        }
    }
}
