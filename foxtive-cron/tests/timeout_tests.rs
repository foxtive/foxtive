mod common;
use foxtive_cron::contracts::{JobContract, ValidatedSchedule, Schedule};
use foxtive_cron::{JobItem, CronError};
use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;
use async_trait::async_trait;

mod timeouts {
    use super::*;

    #[tokio::test]
    async fn job_timeout_is_enforced() {
        struct SlowJob {
            schedule: ValidatedSchedule,
        }
        #[async_trait]
        impl JobContract for SlowJob {
            async fn run(&self) -> foxtive_cron::CronResult<()> {
                tokio::time::sleep(Duration::from_secs(2)).await;
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> { Cow::Borrowed("slow") }
            fn name(&self) -> Cow<'_, str> { Cow::Borrowed("Slow") }
            fn schedule(&self) -> &dyn Schedule { &self.schedule }
            fn timeout(&self) -> Option<Duration> { Some(Duration::from_secs(1)) }
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
}
