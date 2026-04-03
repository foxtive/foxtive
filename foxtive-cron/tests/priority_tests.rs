mod common;
use async_trait::async_trait;
use foxtive_cron::Cron;
use foxtive_cron::contracts::{JobContract, ValidatedSchedule};
use std::borrow::Cow;

mod priority_tests {
    use super::*;

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
            async fn run(&self) -> foxtive_cron::CronResult<()> {
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
}
