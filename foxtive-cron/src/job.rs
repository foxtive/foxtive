use crate::contracts::{
    JobContract, JobEvent, JobEventListener, JobState, JobStore, JobType, MetricsExporter,
    RetryPolicy,
};
use crate::{CronError, CronResult};
use chrono::{DateTime, Utc};
use std::borrow::Cow;
use std::sync::Arc;
use tokio::time::{sleep, timeout};

/// An internal wrapper around a `JobContract` that caches the parsed schedule
/// and exposes helper methods used by the scheduler.
///
/// Constructed once per registered job via [`JobItem::new`], ensuring the
/// schedule is valid at registration time rather than at execution time.
#[derive(Clone)]
pub struct JobItem {
    job: Arc<dyn JobContract>,
    listeners: Vec<Arc<dyn JobEventListener>>,
    metrics_exporter: Option<Arc<dyn MetricsExporter>>,
    job_store: Option<Arc<dyn JobStore>>,
}

impl std::fmt::Debug for JobItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobItem")
            .field("id", &self.id())
            .field("name", &self.name())
            .field("job_type", &self.job_type())
            .field("listeners_len", &self.listeners.len())
            .field("metrics_exporter", &self.metrics_exporter.is_some())
            .field("job_store", &self.job_store.is_some())
            .finish()
    }
}

impl JobItem {
    /// Wrap a [`JobContract`] implementor.
    ///
    /// The schedule is validated via [`JobContract::schedule`] at this point.
    /// Returns an error if the job's schedule is invalid.
    pub fn new(
        job: Arc<dyn JobContract>,
        listeners: Vec<Arc<dyn JobEventListener>>,
        metrics_exporter: Option<Arc<dyn MetricsExporter>>,
        job_store: Option<Arc<dyn JobStore>>,
    ) -> CronResult<Self> {
        // Eagerly access the schedule to trigger any validation at registration time.
        let _ = job.schedule();
        Ok(JobItem {
            job,
            listeners,
            metrics_exporter,
            job_store,
        })
    }

    /// The job's stable unique identifier.
    #[allow(dead_code)]
    pub fn id(&self) -> Cow<'_, str> {
        self.job.id()
    }

    /// The job's human-readable name.
    pub fn name(&self) -> Cow<'_, str> {
        self.job.name()
    }

    /// The type of job.
    pub fn job_type(&self) -> JobType {
        self.job.job_type()
    }

    /// Computes the next scheduled execution time from now.
    pub fn next_run_time(&self) -> Option<DateTime<Utc>> {
        match self.job.job_type() {
            JobType::Once => {
                let run_at = self.job.run_at()?;
                if run_at > Utc::now() {
                    Some(run_at)
                } else {
                    None
                }
            }
            JobType::Recurring => {
                let mut after = Utc::now();
                if let Some(start_after) = self.job.start_after()
                    && start_after > after
                {
                    after = start_after;
                }
                self.job.schedule().next_after(&after, self.job.timezone())
            }
        }
    }

    /// Computes the next scheduled execution time after a specific time.
    pub fn next_run_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self.job.job_type() {
            JobType::Once => {
                let run_at = self.job.run_at()?;
                if run_at > after { Some(run_at) } else { None }
            }
            JobType::Recurring => {
                let mut after = after;
                if let Some(start_after) = self.job.start_after()
                    && start_after > after
                {
                    after = start_after;
                }
                self.job.schedule().next_after(&after, self.job.timezone())
            }
        }
    }

    /// Returns the job's priority.
    pub fn priority(&self) -> i32 {
        self.job.priority()
    }

    /// Returns the job's concurrency limit.
    pub fn concurrency_limit(&self) -> Option<usize> {
        self.job.concurrency_limit()
    }

    /// Returns the job's misfire policy.
    pub fn misfire_policy(&self) -> crate::contracts::MisfirePolicy {
        self.job.misfire_policy()
    }

    async fn emit_event(&self, event: JobEvent) {
        for listener in &self.listeners {
            listener.on_event(event.clone()).await;
        }
    }

    /// Runs the lifecycle sequence: `on_start` → `run` → `on_complete` / `on_error`.
    /// Handles timeouts and retries internally.
    pub async fn run(&self) -> CronResult<()> {
        let retry_policy = self.job.retry_policy();
        let mut attempts = 0;
        let id = self.id().to_string();
        let name = self.name().to_string();

        let mut state = if let Some(store) = &self.job_store {
            store.get_state(&id).await?.unwrap_or_default()
        } else {
            JobState::default()
        };

        loop {
            self.emit_event(JobEvent::Started {
                id: id.clone(),
                name: name.clone(),
            })
            .await;

            if let Some(exporter) = &self.metrics_exporter {
                exporter.record_start(&id, &name);
            }

            self.job.on_start().await;
            let start_time = Utc::now();
            state.last_run = Some(start_time);

            let result = if let Some(duration) = self.job.timeout() {
                match timeout(duration, self.job.run()).await {
                    Ok(res) => res,
                    Err(_) => Err(CronError::ExecutionError(anyhow::anyhow!(
                        "Job timed out after {:?}",
                        duration
                    ))),
                }
            } else {
                self.job.run().await
            };

            match result {
                Ok(()) => {
                    let end_time = Utc::now();
                    let duration = (end_time - start_time).to_std().unwrap_or_default();

                    state.last_success = Some(end_time);
                    state.consecutive_failures = 0;
                    if let Some(store) = &self.job_store {
                        store.save_state(&id, &state).await?;
                    }

                    self.emit_event(JobEvent::Completed {
                        id: id.clone(),
                        name: name.clone(),
                        duration,
                    })
                    .await;

                    if let Some(exporter) = &self.metrics_exporter {
                        exporter.record_completion(&id, &name, duration);
                    }

                    self.job.on_complete().await;
                    return Ok(());
                }
                Err(err) => {
                    attempts += 1;
                    state.last_failure = Some(Utc::now());
                    state.consecutive_failures += 1;
                    if let Some(store) = &self.job_store {
                        store.save_state(&id, &state).await?;
                    }

                    let should_retry = match &retry_policy {
                        RetryPolicy::None => false,
                        RetryPolicy::Fixed { max_retries, .. } => attempts <= *max_retries,
                        RetryPolicy::Exponential { max_retries, .. } => attempts <= *max_retries,
                    };

                    if should_retry {
                        let delay = match &retry_policy {
                            RetryPolicy::None => std::time::Duration::from_secs(0),
                            RetryPolicy::Fixed { interval, .. } => *interval,
                            RetryPolicy::Exponential {
                                initial_interval,
                                max_interval,
                                ..
                            } => {
                                // Cap the exponent to prevent f64 overflow (2^1023 is near f64::MAX)
                                let exponent = ((attempts as i32 - 1) as u32).min(1023);
                                let backoff =
                                    initial_interval.as_secs_f64() * (2.0f64.powi(exponent as i32));
                                std::time::Duration::from_secs_f64(backoff).min(*max_interval)
                            }
                        };

                        self.emit_event(JobEvent::Retrying {
                            id: id.clone(),
                            name: name.clone(),
                            attempt: attempts,
                            delay,
                        })
                        .await;

                        if let Some(exporter) = &self.metrics_exporter {
                            exporter.record_retry(&id, &name);
                        }

                        tracing::warn!(
                            "[{}] Job failed, retrying in {:?} (attempt {}): {:?}",
                            self.name(),
                            delay,
                            attempts,
                            err
                        );

                        sleep(delay).await;
                        continue;
                    } else {
                        self.emit_event(JobEvent::Failed {
                            id: id.clone(),
                            name: name.clone(),
                            error: err.to_string(),
                        })
                        .await;

                        if let Some(exporter) = &self.metrics_exporter {
                            exporter.record_failure(&id, &name);
                        }

                        self.job.on_error(&err).await;
                        return Err(err);
                    }
                }
            }
        }
    }
}
