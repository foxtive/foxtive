use crate::CronResult;
use crate::contracts::JobContract;
use chrono::{DateTime, Utc};
use std::borrow::Cow;
use std::sync::Arc;

/// An internal wrapper around a `JobContract` that caches the parsed schedule
/// and exposes helper methods used by the scheduler.
///
/// Constructed once per registered job via [`JobItem::new`], ensuring the
/// schedule is valid at registration time rather than at execution time.
#[derive(Clone)]
pub struct JobItem {
    job: Arc<dyn JobContract>,
}

impl JobItem {
    /// Wrap a [`JobContract`] implementor.
    ///
    /// The schedule is validated via [`JobContract::schedule`] at this point.
    /// Returns an error if the job's schedule is invalid.
    pub fn new(job: Arc<dyn JobContract>) -> CronResult<Self> {
        // Eagerly access the schedule to trigger any validation at registration time.
        let _ = job.schedule();
        Ok(JobItem { job })
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

    /// Computes the next scheduled execution time from now.
    pub fn next_run_time(&self) -> Option<DateTime<Utc>> {
        self.job.schedule().0.upcoming(Utc).next()
    }

    /// Runs the lifecycle sequence: `on_start` → `run` → `on_complete` / `on_error`.
    pub async fn run(&self) -> CronResult<()> {
        self.job.on_start().await;
        match self.job.run().await {
            Ok(()) => {
                self.job.on_complete().await;
                Ok(())
            }
            Err(err) => {
                self.job.on_error(&err).await;
                Err(err)
            }
        }
    }
}