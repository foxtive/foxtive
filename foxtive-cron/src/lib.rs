use crate::contracts::JobContract;
pub use crate::job::JobItem;
use chrono::{DateTime, Utc};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;
use tokio::time::{sleep_until, Instant};

pub mod contracts;
mod fn_job;
mod job;

pub use fn_job::FnJob;

/// A type alias for results returned by cron jobs, using `anyhow::Result`.
pub type CronResult<T> = anyhow::Result<T>;

/// Represents a job scheduled to run at a specific time.
///
/// Used internally in a min-heap (`BinaryHeap`) to efficiently track
/// the next job due for execution.
#[derive(Clone)]
struct ScheduledJob {
    /// The next time this job is scheduled to run.
    next_run: DateTime<Utc>,
    /// The job to execute.
    job: JobItem,
}

impl Eq for ScheduledJob {}

impl PartialEq for ScheduledJob {
    fn eq(&self, other: &Self) -> bool {
        self.next_run == other.next_run
    }
}

impl Ord for ScheduledJob {
    /// Reverse ordering so the job with the **earliest `next_run`** is at the top.
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
/// - Adding fully custom jobs via the [`JobContract`] trait.
/// - Registering async closures using [`add_job_fn`](Self::add_job_fn).
/// - Registering blocking closures using [`add_blocking_job_fn`](Self::add_blocking_job_fn).
///
/// Jobs are executed concurrently in separate Tokio tasks and automatically
/// rescheduled after each execution according to their cron schedule.
///
/// ### Example
/// ```no_run
/// use foxtive_cron::Cron;
///
/// #[tokio::main]
/// async fn main() {
///     let mut cron = Cron::new();
///
///     cron.add_job_fn("ping", "Ping", "*/5 * * * * * *", || async {
///         println!("Ping at {}", chrono::Utc::now());
///         Ok(())
///     }).unwrap();
///
///     tokio::spawn(async move { cron.run().await });
/// }
/// ```
pub struct Cron {
    queue: BinaryHeap<ScheduledJob>,
}

impl Cron {
    /// Creates a new empty `Cron` scheduler.
    pub fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
        }
    }

    /// Adds a custom job that implements the [`JobContract`] trait.
    ///
    /// This is the most flexible way to schedule complex job types.
    ///
    /// # Errors
    /// Returns an error if the job's schedule expression is invalid.
    pub fn add_job(&mut self, job: Arc<dyn JobContract>) -> CronResult<()> {
        let job = JobItem::new(job)?;
        if let Some(next_run) = job.next_run_time() {
            self.queue.push(ScheduledJob { next_run, job });
        }
        Ok(())
    }

    /// Adds a job from an asynchronous closure or `async fn`.
    ///
    /// # Parameters
    /// - `id`: A stable unique identifier for this job.
    /// - `name`: A human-readable label used in logs.
    /// - `schedule_expr`: A cron expression string.
    /// - `func`: An async closure or function to run at the scheduled time.
    ///
    /// # Errors
    /// Returns an error if `schedule_expr` is not a valid cron expression.
    ///
    /// # Example
    /// ```rust
    /// use foxtive_cron::Cron;
    ///
    /// let mut cron = Cron::new();
    /// cron.add_job_fn("heartbeat", "Heartbeat", "*/10 * * * * * *", || async {
    ///     println!("Heartbeat ping");
    ///     Ok(())
    /// }).unwrap();
    /// ```
    pub fn add_job_fn<F, Fut>(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        schedule_expr: &str,
        func: F,
    ) -> CronResult<()>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = CronResult<()>> + Send + 'static,
    {
        let job = Arc::new(FnJob::new(id, name, schedule_expr, func)?);
        self.add_job(job)
    }

    /// Adds a job from a **blocking** closure or function.
    ///
    /// Useful for CPU-heavy or synchronous operations such as file I/O, backups,
    /// or compression. The function is executed via `tokio::task::spawn_blocking`.
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
    /// use foxtive_cron::Cron;
    ///
    /// let mut cron = Cron::new();
    /// cron.add_blocking_job_fn("compress", "Compress", "0 0 * * * * *", || {
    ///     std::thread::sleep(std::time::Duration::from_secs(1));
    ///     Ok(())
    /// }).unwrap();
    /// ```
    pub fn add_blocking_job_fn<F>(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        schedule_expr: &str,
        func: F,
    ) -> CronResult<()>
    where
        F: Fn() -> CronResult<()> + Send + Sync + 'static + Clone,
    {
        let job = Arc::new(FnJob::new_blocking(id, name, schedule_expr, func)?);
        self.add_job(job)
    }

    /// Starts the scheduler loop.
    ///
    /// 1. **Peeks** at the soonest job without removing it.
    /// 2. Sleeps until that job's scheduled time.
    /// 3. **Drains all jobs whose `next_run` is now due** — so multiple jobs
    ///    scheduled at the same tick fire concurrently.
    /// 4. Spawns each due job as an independent Tokio task.
    /// 5. Re-queues each job for its next occurrence.
    ///
    /// Should be spawned via `tokio::spawn` or awaited directly.
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
        loop {
            // Peek at the next job without removing it.
            let next_run = match self.queue.peek() {
                Some(scheduled) => scheduled.next_run,
                None => {
                    tracing::warn!("Cron queue is empty, scheduler exiting");
                    return;
                }
            };

            let now = Utc::now();
            if next_run > now {
                let delay = (next_run - now).to_std().unwrap_or_default();
                sleep_until(Instant::now() + delay).await;
            }

            // Drain all jobs that are due now (handles multiple jobs at the same tick).
            let now = Utc::now();
            let mut due_jobs = Vec::new();
            while let Some(scheduled) = self.queue.peek() {
                if scheduled.next_run <= now {
                    // Safe to unwrap: we just peeked successfully.
                    due_jobs.push(self.queue.pop().unwrap());
                } else {
                    break;
                }
            }

            // Spawn each due job concurrently and re-queue for its next run.
            for scheduled in due_jobs {
                let job = scheduled.job.clone();
                let name = job.name().to_string();

                tokio::spawn(async move {
                    tracing::info!("[{name}] Running job");
                    match job.run().await {
                        Ok(()) => tracing::info!("[{name}] Job completed"),
                        Err(err) => tracing::error!("[{name}] Job failed: {err:?}"),
                    }
                });

                // Re-schedule for the next occurrence.
                if let Some(next_run) = scheduled.job.next_run_time() {
                    self.queue.push(ScheduledJob {
                        next_run,
                        job: scheduled.job,
                    });
                }
            }
        }
    }
}
