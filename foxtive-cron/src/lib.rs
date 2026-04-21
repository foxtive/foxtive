use crate::contracts::{
    JobContract, JobEvent, JobEventListener, JobStore, JobType, MetricsExporter, MisfirePolicy,
};
pub use crate::job::JobItem;
use chrono::{DateTime, Utc};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::{Instant, sleep_until};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

pub mod contracts;
mod fn_job;
mod job;
pub mod builder;

pub use fn_job::FnJob;
pub use builder::CronExpression;

/// Custom error types for the `foxtive-cron` library.
#[derive(Debug, Error)]
pub enum CronError {
    #[error("Invalid cron expression: {0}")]
    InvalidSchedule(String),

    #[error("Job not found: {0}")]
    JobNotFound(String),

    #[error("Job execution failed: {0}")]
    ExecutionError(#[from] anyhow::Error),

    #[error("Task join error: {0}")]
    JoinError(#[from] tokio::task::JoinError),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Scheduler is shutting down")]
    ShuttingDown,

    #[error("Persistence error: {0}")]
    PersistenceError(String),
}

/// A type alias for results returned by cron jobs, using [`CronError`].
pub type CronResult<T> = Result<T, CronError>;

/// Represents a job scheduled to run at a specific time.
///
/// Used internally in a min-heap (`BinaryHeap`) to efficiently track
/// the next job due for execution.
#[derive(Clone, Debug)]
struct ScheduledJob {
    /// The next time this job is scheduled to run.
    next_run: DateTime<Utc>,
    /// The job's priority.
    priority: i32,
    /// The job ID to identify which job in the registry to execute.
    id: String,
}

impl Eq for ScheduledJob {}

impl PartialEq for ScheduledJob {
    fn eq(&self, other: &Self) -> bool {
        self.next_run == other.next_run && self.priority == other.priority
    }
}

impl Ord for ScheduledJob {
    /// Reverse ordering so the job with the **earliest `next_run`** is at the top.
    /// If `next_run` is the same, use `priority` as a tie-breaker (higher priority first).
    fn cmp(&self, other: &Self) -> Ordering {
        match other.next_run.cmp(&self.next_run) {
            Ordering::Equal => self.priority.cmp(&other.priority),
            ord => ord,
        }
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
pub struct Cron {
    queue: BinaryHeap<ScheduledJob>,
    registry: HashMap<String, JobItem>,
    global_concurrency_limit: Option<Arc<Semaphore>>,
    per_job_semaphores: HashMap<String, Arc<Semaphore>>,
    listeners: Vec<Arc<dyn JobEventListener>>,
    metrics_exporter: Option<Arc<dyn MetricsExporter>>,
    job_store: Option<Arc<dyn JobStore>>,
    shutdown_token: CancellationToken,
    tasks: JoinSet<()>,
    /// Track which jobs have been removed but may still be running
    removed_jobs: HashSet<String>,
}

impl std::fmt::Debug for Cron {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cron")
            .field("queue_len", &self.queue.len())
            .field("registry_len", &self.registry.len())
            .field(
                "global_concurrency_limit",
                &self.global_concurrency_limit.is_some(),
            )
            .field("per_job_semaphores_len", &self.per_job_semaphores.len())
            .field("listeners_len", &self.listeners.len())
            .field("metrics_exporter", &self.metrics_exporter.is_some())
            .field("job_store", &self.job_store.is_some())
            .field(
                "shutdown_token_cancelled",
                &self.shutdown_token.is_cancelled(),
            )
            .field("removed_jobs_count", &self.removed_jobs.len())
            .finish()
    }
}

/// A builder for the `Cron` scheduler.
#[derive(Default)]
pub struct CronBuilder {
    global_concurrency_limit: Option<usize>,
    listeners: Vec<Arc<dyn JobEventListener>>,
    metrics_exporter: Option<Arc<dyn MetricsExporter>>,
    job_store: Option<Arc<dyn JobStore>>,
}

impl std::fmt::Debug for CronBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CronBuilder")
            .field("global_concurrency_limit", &self.global_concurrency_limit)
            .field("listeners_len", &self.listeners.len())
            .field("metrics_exporter", &self.metrics_exporter.is_some())
            .field("job_store", &self.job_store.is_some())
            .finish()
    }
}

impl CronBuilder {
    /// Creates a new `CronBuilder`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a global concurrency limit for the scheduler.
    pub fn with_global_concurrency_limit(mut self, limit: usize) -> Self {
        self.global_concurrency_limit = Some(limit);
        self
    }

    /// Adds an event listener to the scheduler.
    pub fn with_listener(mut self, listener: Arc<dyn JobEventListener>) -> Self {
        self.listeners.push(listener);
        self
    }

    /// Sets a metrics exporter for the scheduler.
    pub fn with_metrics_exporter(mut self, exporter: Arc<dyn MetricsExporter>) -> Self {
        self.metrics_exporter = Some(exporter);
        self
    }

    /// Sets a job store for the scheduler.
    pub fn with_job_store(mut self, store: Arc<dyn JobStore>) -> Self {
        self.job_store = Some(store);
        self
    }

    /// Builds the `Cron` scheduler.
    pub fn build(self) -> Cron {
        let mut cron = Cron::new();
        if let Some(limit) = self.global_concurrency_limit {
            cron = cron.with_global_concurrency_limit(limit);
        }
        cron.listeners = self.listeners;
        cron.metrics_exporter = self.metrics_exporter;
        cron.job_store = self.job_store;
        cron
    }
}

impl Cron {
    /// Creates a new empty `Cron` scheduler.
    pub fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
            registry: HashMap::new(),
            global_concurrency_limit: None,
            per_job_semaphores: HashMap::new(),
            listeners: Vec::new(),
            metrics_exporter: None,
            job_store: None,
            shutdown_token: CancellationToken::new(),
            tasks: JoinSet::new(),
            removed_jobs: HashSet::new(),
        }
    }

    /// Creates a `CronBuilder` for configuring the scheduler.
    pub fn builder() -> CronBuilder {
        CronBuilder::new()
    }

    /// Sets a global concurrency limit for the scheduler.
    pub fn with_global_concurrency_limit(mut self, limit: usize) -> Self {
        self.global_concurrency_limit = Some(Arc::new(Semaphore::new(limit)));
        self
    }

    /// Adds an event listener to the scheduler.
    pub fn add_listener(&mut self, listener: Arc<dyn JobEventListener>) {
        self.listeners.push(listener);
    }

    /// Sets a metrics exporter for the scheduler.
    pub fn set_metrics_exporter(&mut self, exporter: Arc<dyn MetricsExporter>) {
        self.metrics_exporter = Some(exporter);
    }

    /// Sets a job store for the scheduler.
    pub fn set_job_store(&mut self, store: Arc<dyn JobStore>) {
        self.job_store = Some(store);
    }

    /// Adds a custom job that implements the [`JobContract`] trait.
    ///
    /// This is the most flexible way to schedule complex job types.
    ///
    /// # Errors
    /// Returns an error if the job's schedule expression is invalid.
    pub fn add_job(&mut self, job: impl JobContract + 'static) -> CronResult<()> {
        let job_item = JobItem::new(
            Arc::new(job),
            self.listeners.clone(),
            self.metrics_exporter.clone(),
            self.job_store.clone(),
        )?;
        let id = job_item.id().to_string();

        if let Some(limit) = job_item.concurrency_limit() {
            self.per_job_semaphores
                .insert(id.clone(), Arc::new(Semaphore::new(limit)));
        }

        if let Some(next_run) = job_item.next_run_time() {
            self.queue.push(ScheduledJob {
                next_run,
                priority: job_item.priority(),
                id: id.clone(),
            });
        }

        self.registry.insert(id, job_item);
        Ok(())
    }

    /// Adds a job from an asynchronous closure or `async fn`.
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
        let job = FnJob::new(id, name, schedule_expr, func)?;
        self.add_job(job)
    }

    /// Adds a job from a **blocking** closure or function.
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
        let job = FnJob::new_blocking(id, name, schedule_expr, func)?;
        self.add_job(job)
    }

    /// Removes a job from the scheduler by its ID.
    ///
    /// Note: This does not stop already running instances of the job,
    /// but prevents future executions from being scheduled.
    /// The semaphore for this job will be kept until all running instances complete.
    pub fn remove_job(&mut self, id: &str) -> Option<JobItem> {
        // Mark as removed so we know not to reschedule it
        self.removed_jobs.insert(id.to_string());

        // Remove from registry but keep semaphore for running instances
        self.registry.remove(id)
    }

    async fn emit_event(&self, event: JobEvent) {
        for listener in &self.listeners {
            listener.on_event(event.clone()).await;
        }
    }

    /// Triggers a job to run immediately.
    ///
    /// This does not affect the job's regular schedule.
    pub async fn trigger_job(&mut self, id: &str) -> CronResult<()> {
        if self.shutdown_token.is_cancelled() {
            return Err(CronError::ShuttingDown);
        }

        if let Some(job_item) = self.registry.get(id) {
            let job_item_to_spawn = job_item.clone();
            let name = job_item.name().to_string();

            let global_semaphore = self.global_concurrency_limit.clone();
            let job_semaphore = self.per_job_semaphores.get(id).cloned();

            let _scheduler_weak = Arc::new(()); // Dummy for now, we need a real weak ref to Cron if we want to remove Once jobs properly

            self.tasks.spawn(async move {
                let _global_permit = match global_semaphore {
                    Some(sem) => match sem.acquire_owned().await {
                        Ok(permit) => Some(permit),
                        Err(_) => return, // Semaphore closed
                    },
                    None => None,
                };

                let _job_permit = match job_semaphore {
                    Some(sem) => match sem.acquire_owned().await {
                        Ok(permit) => Some(permit),
                        Err(_) => return, // Semaphore closed
                    },
                    None => None,
                };

                info!("[{name}] Manually triggering job");
                match job_item_to_spawn.run().await {
                    Ok(()) => info!("[{name}] Manual job completed"),
                    Err(err) => error!("[{name}] Manual job failed: {err:?}"),
                }
            });
            Ok(())
        } else {
            Err(CronError::JobNotFound(id.to_string()))
        }
    }

    /// Returns a list of IDs for all registered jobs.
    pub fn list_job_ids(&self) -> Vec<String> {
        self.registry.keys().cloned().collect()
    }

    /// Returns the number of jobs currently in the queue.
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// Returns the next job in the queue without removing it.
    pub fn peek_job_id(&self) -> Option<String> {
        self.queue.peek().map(|j| j.id.clone())
    }

    /// Signals the scheduler to stop and waits for all active jobs to finish.
    pub async fn shutdown(&mut self) {
        info!("Shutting down cron scheduler...");
        self.shutdown_token.cancel();
        while let Some(res) = self.tasks.join_next().await {
            if let Err(err) = res {
                error!("Error joining task during shutdown: {:?}", err);
            }
        }
        info!("Cron scheduler shutdown complete.");
    }

    /// Starts the scheduler loop.
    pub async fn run(&mut self) {
        loop {
            // Cleanup finished tasks from JoinSet to prevent memory leak
            while let Some(result) = self.tasks.try_join_next() {
                if let Err(err) = result {
                    error!("Error in scheduled task: {:?}", err);
                }
            }

            // Clean up semaphores for removed jobs that are no longer running
            self.cleanup_removed_job_semaphores();

            // Peek at the next job without removing it.
            let (next_run, id) = match self.queue.peek() {
                Some(scheduled) => (scheduled.next_run, scheduled.id.clone()),
                None => {
                    warn!("Cron queue is empty, scheduler exiting");
                    return;
                }
            };

            // If the job was removed from registry, pop it from queue and continue.
            if !self.registry.contains_key(&id) {
                self.queue.pop();
                continue;
            }

            let now = Utc::now();
            if next_run > now {
                let delay = (next_run - now).to_std().unwrap_or_default();
                tokio::select! {
                    _ = sleep_until(Instant::now() + delay) => {}
                    _ = self.shutdown_token.cancelled() => {
                        return;
                    }
                }
            }

            if self.shutdown_token.is_cancelled() {
                return;
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
                // Only run if it still exists in registry
                if let Some(job_item) = self.registry.get(&scheduled.id) {
                    let job_item_to_spawn = job_item.clone();
                    let name = job_item.name().to_string();

                    let global_semaphore = self.global_concurrency_limit.clone();
                    let job_semaphore = self.per_job_semaphores.get(&scheduled.id).cloned();

                    let scheduled_time = scheduled.next_run;

                    let name_cloned = name.clone();
                    let id_cloned = scheduled.id.clone();
                    let is_once_job = job_item.job_type() == JobType::Once;

                    self.tasks.spawn(async move {
                        let _global_permit = match global_semaphore {
                            Some(sem) => match sem.acquire_owned().await {
                                Ok(permit) => Some(permit),
                                Err(_) => return, // Semaphore closed
                            },
                            None => None,
                        };

                        let _job_permit = match job_semaphore {
                            Some(sem) => match sem.acquire_owned().await {
                                Ok(permit) => Some(permit),
                                Err(_) => return, // Semaphore closed
                            },
                            None => None,
                        };

                        info!("[{name}] Running job");
                        match job_item_to_spawn.run().await {
                            Ok(()) => info!("[{name}] Job completed"),
                            Err(err) => error!("[{name}] Job failed: {err:?}"),
                        }

                        // Permits are automatically returned when dropped
                    });

                    // One-time jobs should be marked for removal after they complete.
                    // We defer actual cleanup until after the job completes.
                    if is_once_job {
                        self.removed_jobs.insert(id_cloned);
                    }

                    // Re-schedule based on misfire policy
                    let now = Utc::now();
                    let misfire_policy = job_item.misfire_policy();

                    if scheduled_time < now {
                        self.emit_event(JobEvent::Misfired {
                            id: scheduled.id.clone(),
                            name: name_cloned.clone(),
                            scheduled_time,
                        })
                        .await;

                        if let Some(exporter) = &self.metrics_exporter {
                            exporter.record_misfire(&scheduled.id, &name_cloned);
                        }
                    }

                    match misfire_policy {
                        MisfirePolicy::Skip => {
                            if let Some(next_run) = job_item.next_run_time() {
                                self.queue.push(ScheduledJob {
                                    next_run,
                                    priority: job_item.priority(),
                                    id: scheduled.id,
                                });
                            }
                        }
                        MisfirePolicy::FireOnce => {
                            // If we're behind schedule, fire once as soon as possible.
                            // The next execution after 'now' will resume regular schedule.
                            if let Some(next_run) = job_item.next_run_time() {
                                self.queue.push(ScheduledJob {
                                    next_run,
                                    priority: job_item.priority(),
                                    id: scheduled.id,
                                });
                            }
                        }
                        MisfirePolicy::FireAll => {
                            // Find the very next occurrence after the one we just processed
                            if let Some(next) = job_item.next_run_after(scheduled_time) {
                                self.queue.push(ScheduledJob {
                                    next_run: next,
                                    priority: job_item.priority(),
                                    id: scheduled.id,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    /// Clean up semaphores for jobs that have been removed and are no longer running.
    fn cleanup_removed_job_semaphores(&mut self) {
        // Find jobs that are both removed AND no longer running
        let mut to_cleanup = Vec::new();
        for job_id in &self.removed_jobs {
            // Check if job is still in registry (running one-time jobs)
            if !self.registry.contains_key(job_id) {
                // Check if semaphore exists (might have been cleaned already)
                if self.per_job_semaphores.contains_key(job_id) {
                    to_cleanup.push(job_id.clone());
                }
            }
        }

        // Clean up semaphores for fully completed removed jobs
        for job_id in to_cleanup {
            self.per_job_semaphores.remove(&job_id);
            self.removed_jobs.remove(&job_id);
        }
    }
}
