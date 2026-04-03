mod common;
use common::*;
use foxtive_cron::JobItem;
use foxtive_cron::contracts::RetryPolicy;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

mod retries {
    use super::*;

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
