use anyhow::anyhow;
use async_trait::async_trait;
use foxtive_cron::contracts::{JobContract, ValidatedSchedule};
use foxtive_cron::{Cron, CronResult, FnJob, JobItem};
use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::time::{Duration, timeout};

// Helpers

/// Minimal hand-rolled `JobContract` used throughout the tests.
struct MockJob {
    id: &'static str,
    name: &'static str,
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
}

impl MockJob {
    fn new(id: &'static str, schedule_expr: &str) -> Self {
        Self {
            id,
            name: id,
            schedule: ValidatedSchedule::parse(schedule_expr).unwrap(),
            run_count: Arc::new(AtomicUsize::new(0)),
            should_fail: false,
            start_count: Arc::new(AtomicUsize::new(0)),
            complete_count: Arc::new(AtomicUsize::new(0)),
            error_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn failing(id: &'static str, schedule_expr: &str) -> Self {
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
            Err(anyhow!("intentional failure"))
        } else {
            Ok(())
        }
    }

    fn id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id)
    }

    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.name)
    }

    fn schedule(&self) -> &ValidatedSchedule {
        &self.schedule
    }

    async fn on_start(&self) {
        self.start_count.fetch_add(1, Ordering::SeqCst);
    }

    async fn on_complete(&self) {
        self.complete_count.fetch_add(1, Ordering::SeqCst);
    }

    async fn on_error(&self, _error: &anyhow::Error) {
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
            Err(anyhow!("oops"))
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
            Err(anyhow!("blocked error"))
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

        let item = JobItem::new(mock).unwrap();
        item.run().await.unwrap();

        assert_eq!(start_count.load(Ordering::SeqCst), 1);
        assert_eq!(run_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn on_complete_called_on_success() {
        let mock = Arc::new(MockJob::new("job", "*/1 * * * * * *"));
        let complete_count = mock.complete_count.clone();
        let error_count = mock.error_count.clone();

        let item = JobItem::new(mock).unwrap();
        item.run().await.unwrap();

        assert_eq!(complete_count.load(Ordering::SeqCst), 1);
        assert_eq!(error_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn on_error_called_on_failure() {
        let mock = Arc::new(MockJob::failing("job", "*/1 * * * * * *"));
        let complete_count = mock.complete_count.clone();
        let error_count = mock.error_count.clone();

        let item = JobItem::new(mock).unwrap();
        let result = item.run().await;

        assert!(result.is_err());
        assert_eq!(error_count.load(Ordering::SeqCst), 1);
        assert_eq!(complete_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn on_complete_not_called_on_failure() {
        let mock = Arc::new(MockJob::failing("job", "*/1 * * * * * *"));
        let complete_count = mock.complete_count.clone();

        let item = JobItem::new(mock).unwrap();
        let _ = item.run().await;

        assert_eq!(complete_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn on_error_not_called_on_success() {
        let mock = Arc::new(MockJob::new("job", "*/1 * * * * * *"));
        let error_count = mock.error_count.clone();

        let item = JobItem::new(mock).unwrap();
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
        }

        let log = Arc::new(Mutex::new(Vec::new()));
        let job = Arc::new(OrderedJob {
            schedule: ValidatedSchedule::parse("*/1 * * * * * *").unwrap(),
            log: log.clone(),
        });

        let item = JobItem::new(job).unwrap();
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
        let job = Arc::new(MockJob::new("mock", "*/1 * * * * * *"));
        assert!(cron.add_job(job).is_ok());
    }

    #[test]
    fn multiple_jobs_can_be_registered() {
        let mut cron = Cron::new();
        for i in 0..5 {
            let job = Arc::new(MockJob::new("mock", "*/1 * * * * * *"));
            cron.add_job(job)
                .unwrap_or_else(|_| panic!("failed on job {i}"));
        }
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
            Err(anyhow!("always fails"))
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
}
