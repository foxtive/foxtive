mod common;
use common::*;
use foxtive_cron::contracts::{JobContract, ValidatedSchedule, Schedule};
use foxtive_cron::{JobItem, CronError};
use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::Ordering;

mod lifecycle_hooks {
    use foxtive_cron::CronResult;
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

        #[async_trait::async_trait]
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
            fn schedule(&self) -> &dyn Schedule {
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
