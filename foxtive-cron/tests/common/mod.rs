use async_trait::async_trait;
use foxtive_cron::contracts::{
    JobContract, MisfirePolicy, RetryPolicy, Schedule, ValidatedSchedule,
};
use foxtive_cron::{CronError, CronResult};
use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

#[allow(dead_code)]
/// Minimal hand-rolled `JobContract` used throughout the tests.
pub struct MockJob {
    pub id: String,
    pub name: String,
    pub schedule: ValidatedSchedule,
    /// Incremented every time `run` is called.
    pub run_count: Arc<AtomicUsize>,
    /// When `true`, `run` returns an error instead of `Ok(())`.
    pub should_fail: bool,
    /// Incremented every time `on_start` is called.
    pub start_count: Arc<AtomicUsize>,
    /// Incremented every time `on_complete` is called.
    pub complete_count: Arc<AtomicUsize>,
    /// Incremented every time `on_error` is called.
    pub error_count: Arc<AtomicUsize>,
    pub timeout: Option<Duration>,
    pub priority: i32,
    pub concurrency_limit: Option<usize>,
    pub misfire_policy: MisfirePolicy,
    pub retry_policy: RetryPolicy,
}

#[allow(dead_code)]
impl MockJob {
    pub fn new(id: impl Into<String>, schedule_expr: &str) -> Self {
        let id = id.into();
        Self {
            id: id.clone(),
            name: id,
            schedule: ValidatedSchedule::parse(schedule_expr).unwrap(),
            run_count: Arc::new(AtomicUsize::new(0)),
            should_fail: false,
            start_count: Arc::new(AtomicUsize::new(0)),
            complete_count: Arc::new(AtomicUsize::new(0)),
            error_count: Arc::new(AtomicUsize::new(0)),
            timeout: None,
            priority: 0,
            concurrency_limit: None,
            misfire_policy: MisfirePolicy::Skip,
            retry_policy: RetryPolicy::None,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_concurrency_limit(mut self, limit: usize) -> Self {
        self.concurrency_limit = Some(limit);
        self
    }

    pub fn with_misfire_policy(mut self, policy: MisfirePolicy) -> Self {
        self.misfire_policy = policy;
        self
    }

    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    pub fn failing(id: impl Into<String>, schedule_expr: &str) -> Self {
        Self {
            should_fail: true,
            ..Self::new(id, schedule_expr)
        }
    }
}

#[async_trait]
impl JobContract for MockJob {
    async fn run(&self) -> CronResult<()> {
        self.run_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.should_fail {
            Err(CronError::ExecutionError(anyhow::anyhow!(
                "intentional failure"
            )))
        } else {
            Ok(())
        }
    }

    fn id(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.id)
    }

    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.name)
    }

    fn schedule(&self) -> &dyn Schedule {
        &self.schedule
    }

    fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    fn priority(&self) -> i32 {
        self.priority
    }

    fn concurrency_limit(&self) -> Option<usize> {
        self.concurrency_limit
    }

    fn misfire_policy(&self) -> MisfirePolicy {
        self.misfire_policy
    }

    fn retry_policy(&self) -> RetryPolicy {
        self.retry_policy.clone()
    }

    async fn on_start(&self) {
        self.start_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    async fn on_complete(&self) {
        self.complete_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    async fn on_error(&self, _error: &CronError) {
        self.error_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}
