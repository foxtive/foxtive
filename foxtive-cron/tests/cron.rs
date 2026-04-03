use async_trait::async_trait;
use foxtive_cron::contracts::{JobContract, MisfirePolicy, RetryPolicy, ValidatedSchedule};
use foxtive_cron::{Cron, CronError, CronResult, FnJob, JobItem};
use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::timeout;

#[allow(dead_code)]
/// Minimal hand-rolled `JobContract` used throughout the tests.
struct MockJob {
    id: String,
    name: String,
    schedule: ValidatedSchedule,
    /// Incremented every time `run` is called.
    run_count: Arc<AtomicUsize>,
    /// When `true`, `run` returns an error instead of `Ok(())`.
    should_fail: bool,
    /// Incremented every time `on_start` is called.
    start_count: Arc<AtomicUsize>,
    /// Incremented every time `on_complete` is called.
    complete_count: Arc<AtomicUsize>,
    /// Incremented every time `on_error` is called.
    error_count: Arc<AtomicUsize>,
    timeout: Option<Duration>,
    priority: i32,
    concurrency_limit: Option<usize>,
    misfire_policy: MisfirePolicy,
    retry_policy: RetryPolicy,
}

#[allow(dead_code)]
impl MockJob {
    fn new(id: impl Into<String>, schedule_expr: &str) -> Self {
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

    fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    fn with_concurrency_limit(mut self, limit: usize) -> Self {
        self.concurrency_limit = Some(limit);
        self
    }

    fn with_misfire_policy(mut self, policy: MisfirePolicy) -> Self {
        self.misfire_policy = policy;
        self
    }

    fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    fn failing(id: impl Into<String>, schedule_expr: &str) -> Self {
        Self {
            should_fail: true,
            ..Self::new(id, schedule_expr)
        }
    }
}

#[async_trait]
impl JobContract for MockJob {
    async fn run(&self) -> CronResult<()> {
        self.run_count.fetch_add(1, Ordering::SeqCst);
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

    fn schedule(&self) -> &ValidatedSchedule {
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
        self.start_count.fetch_add(1, Ordering::SeqCst);
    }

    async fn on_complete(&self) {
        self.complete_count.fetch_add(1, Ordering::SeqCst);
    }

    async fn on_error(&self, _error: &CronError) {
        self.error_count.fetch_add(1, Ordering::SeqCst);
    }
}

// ValidatedSchedule

mod validated_schedule {
    use super::*;

    #[test]
    fn accepts_valid_seven_field_expression() {
        assert!(ValidatedSchedule::parse("*/5 * * * * * *").is_ok());
    }

    #[test]
    fn accepts_every_second_expression() {
        assert!(ValidatedSchedule::parse("* * * * * * *").is_ok());
    }

    #[test]
    fn accepts_specific_time_expression() {
        // Every day at 03:30:00
        assert!(ValidatedSchedule::parse("0 30 3 * * * *").is_ok());
    }

    #[test]
    fn rejects_empty_expression() {
        assert!(ValidatedSchedule::parse("").is_err());
    }

    #[test]
    fn rejects_nonsense_expression() {
        assert!(ValidatedSchedule::parse("not a cron expression").is_err());
    }

    #[test]
    fn rejects_out_of_range_field() {
        // Seconds field goes 0-59; 99 is invalid.
        assert!(ValidatedSchedule::parse("99 * * * * * *").is_err());
    }

    #[test]
    fn error_message_includes_original_expression() {
        let err = ValidatedSchedule::parse("bad expr").unwrap_err();
        assert!(err.to_string().contains("bad expr"));
    }
}

// FnJob

mod fn_job {
    use super::*;

    #[test]
    fn new_returns_ok_with_valid_schedule() {
        let result = FnJob::new("job-id", "Job Name", "*/1 * * * * * *", || async { Ok(()) });
        assert!(result.is_ok());
    }

    #[test]
    fn new_returns_err_with_invalid_schedule() {
        let result = FnJob::new("job-id", "Job Name", "not-valid", || async { Ok(()) });
        assert!(result.is_err());
    }

    #[test]
    fn new_blocking_returns_ok_with_valid_schedule() {
        let result = FnJob::new_blocking("job-id", "Job Name", "*/1 * * * * * *", || Ok(()));
        assert!(result.is_ok());
    }

    #[test]
    fn new_blocking_returns_err_with_invalid_schedule() {
        let result = FnJob::new_blocking("job-id", "Job Name", "bad", || Ok(()));
        assert!(result.is_err());
    }

    #[test]
    fn id_returns_correct_value() {
        let job = FnJob::new("my-id", "My Name", "*/1 * * * * * *", || async { Ok(()) }).unwrap();
        assert_eq!(job.id(), "my-id");
    }

    #[test]
    fn name_returns_correct_value() {
        let job = FnJob::new("my-id", "My Name", "*/1 * * * * * *", || async { Ok(()) }).unwrap();
        assert_eq!(job.name(), "My Name");
    }

    #[test]
    fn description_defaults_to_none() {
        let job = FnJob::new("id", "Name", "*/1 * * * * * *", || async { Ok(()) }).unwrap();
        assert!(job.description().is_none());
    }

    #[tokio::test]
    async fn run_executes_async_closure() {
        let executed = Arc::new(AtomicBool::new(false));
        let executed_clone = executed.clone();

        let job = FnJob::new("id", "Name", "*/1 * * * * * *", move || {
            let flag = executed_clone.clone();
            async move {
                flag.store(true, Ordering::SeqCst);
                Ok(())
            }
        })
        .unwrap();

        job.run().await.unwrap();
        assert!(executed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn run_propagates_async_closure_error() {
        let job = FnJob::new("id", "Name", "*/1 * * * * * *", || async {
            Err(CronError::ExecutionError(anyhow::anyhow!("oops")))
        })
        .unwrap();

        assert!(job.run().await.is_err());
    }

    #[tokio::test]
    async fn run_executes_blocking_closure() {
        let executed = Arc::new(AtomicBool::new(false));
        let executed_clone = executed.clone();

        let job = FnJob::new_blocking("id", "Name", "*/1 * * * * * *", move || {
            executed_clone.store(true, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();

        job.run().await.unwrap();
        assert!(executed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn run_propagates_blocking_closure_error() {
        let job = FnJob::new_blocking("id", "Name", "*/1 * * * * * *", || {
            Err(CronError::ExecutionError(anyhow::anyhow!("blocked error")))
        })
        .unwrap();

        assert!(job.run().await.is_err());
    }
}

// JobContract lifecycle hooks (via MockJob)

mod lifecycle_hooks {
    use super::*;

    #[tokio::test]
    async fn on_start_called_before_run() {
        let mock = Arc::new(MockJob::new("job", "*/1 * * * * * *"));
        let start_count = mock.start_count.clone();
        let run_count = mock.run_count.clone();

        let item = JobItem::new(mock, vec![], None, None).unwrap();
        item.run().await.unwrap();

        assert_eq!(start_count.load(Ordering::SeqCst), 1);
        assert_eq!(run_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn on_complete_called_on_success() {
        let mock = Arc::new(MockJob::new("job", "*/1 * * * * * *"));
        let complete_count = mock.complete_count.clone();
        let error_count = mock.error_count.clone();

        let item = JobItem::new(mock, vec![], None, None).unwrap();
        item.run().await.unwrap();

        assert_eq!(complete_count.load(Ordering::SeqCst), 1);
        assert_eq!(error_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn on_error_called_on_failure() {
        let mock = Arc::new(MockJob::failing("job", "*/1 * * * * * *"));
        let complete_count = mock.complete_count.clone();
        let error_count = mock.error_count.clone();

        let item = JobItem::new(mock, vec![], None, None).unwrap();
        let result = item.run().await;

        assert!(result.is_err());
        assert_eq!(error_count.load(Ordering::SeqCst), 1);
        assert_eq!(complete_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn on_complete_not_called_on_failure() {
        let mock = Arc::new(MockJob::failing("job", "*/1 * * * * * *"));
        let complete_count = mock.complete_count.clone();

        let item = JobItem::new(mock, vec![], None, None).unwrap();
        let _ = item.run().await;

        assert_eq!(complete_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn on_error_not_called_on_success() {
        let mock = Arc::new(MockJob::new("job", "*/1 * * * * * *"));
        let error_count = mock.error_count.clone();

        let item = JobItem::new(mock, vec![], None, None).unwrap();
        item.run().await.unwrap();

        assert_eq!(error_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn all_hooks_fire_in_correct_sequence() {
        // We use a shared counter to verify ordering: start < run < complete.
        use std::sync::Mutex;

        struct OrderedJob {
            schedule: ValidatedSchedule,
            log: Arc<Mutex<Vec<&'static str>>>,
        }

        #[async_trait]
        impl JobContract for OrderedJob {
            async fn run(&self) -> CronResult<()> {
                self.log.lock().unwrap().push("run");
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> {
                Cow::Borrowed("ordered")
            }
            fn name(&self) -> Cow<'_, str> {
                Cow::Borrowed("Ordered")
            }
            fn schedule(&self) -> &ValidatedSchedule {
                &self.schedule
            }
            async fn on_start(&self) {
                self.log.lock().unwrap().push("start");
            }
            async fn on_complete(&self) {
                self.log.lock().unwrap().push("complete");
            }
            async fn on_error(&self, _error: &CronError) {
                self.log.lock().unwrap().push("error");
            }
        }

        let log = Arc::new(Mutex::new(Vec::new()));
        let job = Arc::new(OrderedJob {
            schedule: ValidatedSchedule::parse("*/1 * * * * * *").unwrap(),
            log: log.clone(),
        });

        let item = JobItem::new(job, vec![], None, None).unwrap();
        item.run().await.unwrap();

        let entries = log.lock().unwrap().clone();
        assert_eq!(entries, vec!["start", "run", "complete"]);
    }
}

// Cron scheduler

mod cron_scheduler {
    use super::*;

    #[test]
    fn new_creates_empty_scheduler() {
        let cron = Cron::new();
        // We can only observe emptiness indirectly: adding nothing and
        // checking that `default()` also compiles without panic.
        let _ = cron;
        let _ = Cron::default();
    }

    #[test]
    fn add_job_fn_accepts_valid_schedule() {
        let mut cron = Cron::new();
        let result = cron.add_job_fn("id", "Name", "*/1 * * * * * *", || async { Ok(()) });
        assert!(result.is_ok());
    }

    #[test]
    fn add_job_fn_rejects_invalid_schedule() {
        let mut cron = Cron::new();
        let result = cron.add_job_fn("id", "Name", "bad schedule", || async { Ok(()) });
        assert!(result.is_err());
    }

    #[test]
    fn add_blocking_job_fn_accepts_valid_schedule() {
        let mut cron = Cron::new();
        let result = cron.add_blocking_job_fn("id", "Name", "*/1 * * * * * *", || Ok(()));
        assert!(result.is_ok());
    }

    #[test]
    fn add_blocking_job_fn_rejects_invalid_schedule() {
        let mut cron = Cron::new();
        let result = cron.add_blocking_job_fn("id", "Name", "nope", || Ok(()));
        assert!(result.is_err());
    }

    #[test]
    fn add_job_accepts_arc_job_contract() {
        let mut cron = Cron::new();
        let job = MockJob::new("mock", "*/1 * * * * * *");
        assert!(cron.add_job(job).is_ok());
    }

    #[test]
    fn multiple_jobs_can_be_registered() {
        let mut cron = Cron::new();
        for i in 0..5 {
            let job = MockJob::new(format!("mock-{}", i), "*/1 * * * * * *");
            cron.add_job(job)
                .unwrap_or_else(|_| panic!("failed on job {i}"));
        }
    }

    #[test]
    fn list_job_ids_returns_correct_ids() {
        let mut cron = Cron::new();
        cron.add_job_fn("job-1", "Job 1", "*/1 * * * * * *", || async { Ok(()) })
            .unwrap();
        cron.add_job_fn("job-2", "Job 2", "*/1 * * * * * *", || async { Ok(()) })
            .unwrap();

        let ids = cron.list_job_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"job-1".to_string()));
        assert!(ids.contains(&"job-2".to_string()));
    }

    #[tokio::test]
    async fn remove_job_removes_from_registry() {
        let mut cron = Cron::new();
        cron.add_job_fn("job-1", "Job 1", "*/1 * * * * * *", || async { Ok(()) })
            .unwrap();
        assert!(cron.remove_job("job-1").is_some());
        assert_eq!(cron.list_job_ids().len(), 0);
    }

    #[tokio::test]
    async fn trigger_job_executes_immediately() {
        let run_count = Arc::new(AtomicUsize::new(0));
        let run_count_clone = run_count.clone();

        let mut cron = Cron::new();
        cron.add_job_fn("job-1", "Job 1", "0 0 0 1 1 * *", move || {
            let count = run_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .unwrap();

        cron.trigger_job("job-1").await.unwrap();

        // Wait a bit for the spawned task to run
        let result = timeout(Duration::from_secs(1), async {
            loop {
                if run_count.load(Ordering::SeqCst) >= 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;

        assert!(result.is_ok(), "manual trigger did not execute job");
    }

    /// Verifies that a job registered on a every-second schedule actually fires
    /// within a reasonable window when the scheduler is running.
    #[tokio::test]
    async fn scheduler_executes_registered_job() {
        let run_count = Arc::new(AtomicUsize::new(0));
        let run_count_clone = run_count.clone();

        let mut cron = Cron::new();
        cron.add_job_fn("id", "Name", "*/1 * * * * * *", move || {
            let count = run_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .unwrap();

        let handle = tokio::spawn(async move {
            cron.run().await;
        });

        // Wait up to 3 seconds for at least one execution.
        let result = timeout(Duration::from_secs(3), async {
            loop {
                if run_count.load(Ordering::SeqCst) >= 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;

        handle.abort();
        assert!(result.is_ok(), "job was not executed within 3 seconds");
    }

    /// Verifies that two jobs scheduled every second both fire within a reasonable window.
    #[tokio::test]
    async fn scheduler_executes_multiple_jobs_concurrently() {
        let count_a = Arc::new(AtomicUsize::new(0));
        let count_b = Arc::new(AtomicUsize::new(0));
        let count_a_clone = count_a.clone();
        let count_b_clone = count_b.clone();

        let mut cron = Cron::new();

        cron.add_job_fn("job-a", "Job A", "*/1 * * * * * *", move || {
            let c = count_a_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .unwrap();

        cron.add_job_fn("job-b", "Job B", "*/1 * * * * * *", move || {
            let c = count_b_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .unwrap();

        let handle = tokio::spawn(async move {
            cron.run().await;
        });

        let result = timeout(Duration::from_secs(3), async {
            loop {
                if count_a.load(Ordering::SeqCst) >= 1 && count_b.load(Ordering::SeqCst) >= 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;

        handle.abort();
        assert!(
            result.is_ok(),
            "one or both jobs were not executed within 3 seconds"
        );
    }

    /// A failing job should not crash the scheduler; other jobs must continue firing.
    #[tokio::test]
    async fn failing_job_does_not_crash_scheduler() {
        let good_count = Arc::new(AtomicUsize::new(0));
        let good_count_clone = good_count.clone();

        let mut cron = Cron::new();

        // Register a job that always errors.
        cron.add_job_fn("bad-job", "Bad Job", "*/1 * * * * * *", || async {
            Err(CronError::ExecutionError(anyhow::anyhow!("always fails")))
        })
        .unwrap();

        // Register a healthy job alongside it.
        cron.add_job_fn("good-job", "Good Job", "*/1 * * * * * *", move || {
            let c = good_count_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .unwrap();

        let handle = tokio::spawn(async move {
            cron.run().await;
        });

        let result = timeout(Duration::from_secs(3), async {
            loop {
                if good_count.load(Ordering::SeqCst) >= 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;

        handle.abort();
        assert!(
            result.is_ok(),
            "good job was not executed — scheduler may have crashed on bad job"
        );
    }

    /// An empty scheduler should return promptly rather than hanging forever.
    #[tokio::test]
    async fn empty_scheduler_exits_promptly() {
        let mut cron = Cron::new();
        let result = timeout(Duration::from_millis(200), async move {
            cron.run().await;
        })
        .await;
        assert!(result.is_ok(), "empty scheduler did not exit in time");
    }

    #[tokio::test]
    async fn removed_job_stops_running() {
        let run_count = Arc::new(AtomicUsize::new(0));
        let run_count_clone = run_count.clone();

        let mut cron = Cron::new();
        cron.add_job_fn("job-1", "Job 1", "*/1 * * * * * *", move || {
            let count = run_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .unwrap();

        let run_count_for_loop = run_count.clone();
        let handle = tokio::spawn(async move {
            // Wait for at least one run
            while run_count_for_loop.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            // Start the scheduler in another task
            let (tx, mut rx) = tokio::sync::mpsc::channel(1);
            let mut cron_inner = cron;

            let handle_inner = tokio::spawn(async move {
                tokio::select! {
                    _ = cron_inner.run() => {},
                    _ = rx.recv() => {
                        cron_inner.remove_job("job-1");
                        // We need to keep running to see if it triggers again
                        cron_inner.run().await;
                    },
                }
            });

            // Signal to remove the job after it has run at least once
            tx.send(()).await.unwrap();

            // Let it "run" for a bit more
            tokio::time::sleep(Duration::from_secs(2)).await;
            handle_inner.abort();
        });

        tokio::time::sleep(Duration::from_secs(1)).await;
        let count_after_removal = run_count.load(Ordering::SeqCst);

        tokio::time::sleep(Duration::from_secs(2)).await;
        let final_count = run_count.load(Ordering::SeqCst);

        handle.abort();

        assert!(
            final_count <= count_after_removal + 1,
            "job continued to run after removal. count_after_removal: {}, final_count: {}",
            count_after_removal,
            final_count
        );
    }

    #[tokio::test]
    async fn job_timeout_is_enforced() {
        struct SlowJob {
            schedule: ValidatedSchedule,
        }
        #[async_trait]
        impl JobContract for SlowJob {
            async fn run(&self) -> CronResult<()> {
                tokio::time::sleep(Duration::from_secs(2)).await;
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> {
                Cow::Borrowed("slow")
            }
            fn name(&self) -> Cow<'_, str> {
                Cow::Borrowed("Slow")
            }
            fn schedule(&self) -> &ValidatedSchedule {
                &self.schedule
            }
            fn timeout(&self) -> Option<Duration> {
                Some(Duration::from_secs(1))
            }
            async fn on_error(&self, _error: &CronError) {}
        }

        let job = Arc::new(SlowJob {
            schedule: ValidatedSchedule::parse("*/1 * * * * * *").unwrap(),
        });

        let item = JobItem::new(job, vec![], None, None).unwrap();
        let result = item.run().await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn global_concurrency_limit_is_enforced() {
        let run_count = Arc::new(AtomicUsize::new(0));
        let active_count = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));

        let mut cron = Cron::new().with_global_concurrency_limit(2);

        for i in 0..5 {
            let run_count = run_count.clone();
            let active_count = active_count.clone();
            let max_active = max_active.clone();

            cron.add_job_fn(format!("job-{}", i), "Job", "*/1 * * * * * *", move || {
                let run_count = run_count.clone();
                let active_count = active_count.clone();
                let max_active = max_active.clone();
                async move {
                    let current = active_count.fetch_add(1, Ordering::SeqCst) + 1;
                    loop {
                        let prev = max_active.load(Ordering::SeqCst);
                        if current <= prev
                            || max_active
                                .compare_exchange(prev, current, Ordering::SeqCst, Ordering::SeqCst)
                                .is_ok()
                        {
                            break;
                        }
                    }

                    tokio::time::sleep(Duration::from_millis(100)).await;
                    run_count.fetch_add(1, Ordering::SeqCst);
                    active_count.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .unwrap();
        }

        let handle = tokio::spawn(async move {
            cron.run().await;
        });

        // Wait for some jobs to run
        tokio::time::sleep(Duration::from_secs(2)).await;
        handle.abort();

        assert!(
            max_active.load(Ordering::SeqCst) <= 2,
            "global concurrency limit exceeded: {}",
            max_active.load(Ordering::SeqCst)
        );
        assert!(run_count.load(Ordering::SeqCst) > 0);
    }

    #[tokio::test]
    async fn per_job_concurrency_limit_is_enforced() {
        let run_count = Arc::new(AtomicUsize::new(0));
        let active_count = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));

        let mut cron = Cron::new();
        let run_count_clone = run_count.clone();
        let active_count_clone = active_count.clone();
        let max_active_clone = max_active.clone();

        struct LimitedJob {
            schedule: ValidatedSchedule,
            active_count: Arc<AtomicUsize>,
            max_active: Arc<AtomicUsize>,
            run_count: Arc<AtomicUsize>,
        }
        #[async_trait]
        impl JobContract for LimitedJob {
            async fn run(&self) -> CronResult<()> {
                let current = self.active_count.fetch_add(1, Ordering::SeqCst) + 1;
                loop {
                    let prev = self.max_active.load(Ordering::SeqCst);
                    if current <= prev
                        || self
                            .max_active
                            .compare_exchange(prev, current, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                    {
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
                self.run_count.fetch_add(1, Ordering::SeqCst);
                self.active_count.fetch_sub(1, Ordering::SeqCst);
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> {
                Cow::Borrowed("limited-job")
            }
            fn name(&self) -> Cow<'_, str> {
                Cow::Borrowed("Limited")
            }
            fn schedule(&self) -> &ValidatedSchedule {
                &self.schedule
            }
            fn concurrency_limit(&self) -> Option<usize> {
                Some(1)
            }
            async fn on_error(&self, _error: &CronError) {}
        }

        let job = LimitedJob {
            schedule: ValidatedSchedule::parse("* * * * * * *").unwrap(),
            active_count: active_count_clone,
            max_active: max_active_clone,
            run_count: run_count_clone,
        };

        cron.add_job(job).unwrap();

        let handle = tokio::spawn(async move {
            cron.run().await;
        });

        tokio::time::sleep(Duration::from_secs(2)).await;
        handle.abort();

        assert_eq!(
            max_active.load(Ordering::SeqCst),
            1,
            "per-job concurrency limit exceeded"
        );
    }

    #[tokio::test]
    async fn priority_is_respected() {
        let mut cron = Cron::new();

        struct PriorityJob {
            id: String,
            priority: i32,
            schedule: ValidatedSchedule,
        }
        #[async_trait]
        impl JobContract for PriorityJob {
            async fn run(&self) -> CronResult<()> {
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> {
                Cow::Borrowed(&self.id)
            }
            fn name(&self) -> Cow<'_, str> {
                Cow::Borrowed(&self.id)
            }
            fn schedule(&self) -> &ValidatedSchedule {
                &self.schedule
            }
            fn priority(&self) -> i32 {
                self.priority
            }
            async fn on_error(&self, _error: &CronError) {}
        }

        let schedule = ValidatedSchedule::parse("*/10 * * * * * *").unwrap();

        // Add jobs in "wrong" order
        for p in [1, 3, 2] {
            cron.add_job(PriorityJob {
                id: format!("p-{}", p),
                priority: p,
                schedule: schedule.clone(),
            })
            .unwrap();
        }

        assert_eq!(cron.queue_len(), 3);
        assert_eq!(cron.peek_job_id(), Some("p-3".to_string()));
    }

    #[tokio::test]
    async fn fixed_retry_policy_is_enforced() {
        let job = Arc::new(
            MockJob::failing("retry-job", "*/1 * * * * * *").with_retry_policy(
                RetryPolicy::Fixed {
                    max_retries: 2,
                    interval: Duration::from_millis(10),
                },
            ),
        );

        let run_count_clone = job.run_count.clone();
        let item = JobItem::new(job, vec![], None, None).unwrap();
        let result = item.run().await;

        assert!(result.is_err());
        assert_eq!(run_count_clone.load(Ordering::SeqCst), 3); // 1 original + 2 retries
    }

    #[tokio::test]
    async fn exponential_retry_policy_is_enforced() {
        let job = Arc::new(
            MockJob::failing("retry-job-exp", "*/1 * * * * * *").with_retry_policy(
                RetryPolicy::Exponential {
                    max_retries: 2,
                    initial_interval: Duration::from_millis(10),
                    max_interval: Duration::from_millis(100),
                },
            ),
        );

        let run_count_clone = job.run_count.clone();
        let item = JobItem::new(job, vec![], None, None).unwrap();
        let result = item.run().await;

        assert!(result.is_err());
        assert_eq!(run_count_clone.load(Ordering::SeqCst), 3);
    }
}
