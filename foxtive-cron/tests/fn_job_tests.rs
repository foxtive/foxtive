use foxtive_cron::{CronError, FnJob};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

mod fn_job {
    use super::*;
    use foxtive_cron::contracts::JobContract;

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
