use async_trait::async_trait;
use foxtive_cron::contracts::{JobContract, JobType, MisfirePolicy, Schedule, ValidatedSchedule};
use foxtive_cron::{Cron, CronResult};
use std::borrow::Cow;

mod job_type {
    use super::*;
    use chrono::{Duration, Utc};

    struct OnceJob {
        schedule: ValidatedSchedule,
        run_at: chrono::DateTime<Utc>,
    }

    #[async_trait]
    impl JobContract for OnceJob {
        async fn run(&self) -> CronResult<()> {
            Ok(())
        }
        fn id(&self) -> Cow<'_, str> {
            Cow::Borrowed("once-job")
        }
        fn name(&self) -> Cow<'_, str> {
            Cow::Borrowed("Once Job")
        }
        fn schedule(&self) -> &dyn Schedule {
            &self.schedule
        }
        fn job_type(&self) -> JobType {
            JobType::Once
        }
        fn run_at(&self) -> Option<chrono::DateTime<Utc>> {
            Some(self.run_at)
        }
    }

    #[test]
    fn once_job_type_is_correct() {
        let job = OnceJob {
            schedule: ValidatedSchedule::parse("*/5 * * * * * *").unwrap(),
            run_at: Utc::now() + Duration::seconds(10),
        };
        assert_eq!(job.job_type(), JobType::Once);
    }

    #[test]
    fn recurring_job_type_is_default() {
        struct RecurringJob {
            schedule: ValidatedSchedule,
        }

        #[async_trait]
        impl JobContract for RecurringJob {
            async fn run(&self) -> CronResult<()> {
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> {
                Cow::Borrowed("recurring")
            }
            fn name(&self) -> Cow<'_, str> {
                Cow::Borrowed("Recurring")
            }
            fn schedule(&self) -> &dyn Schedule {
                &self.schedule
            }
        }

        let job = RecurringJob {
            schedule: ValidatedSchedule::parse("*/5 * * * * * *").unwrap(),
        };
        assert_eq!(job.job_type(), JobType::Recurring);
    }

    #[tokio::test]
    async fn once_job_with_future_run_at_is_scheduled() {
        let mut cron = Cron::new();
        let future_time = Utc::now() + Duration::seconds(30);

        let job = OnceJob {
            schedule: ValidatedSchedule::parse("* * * * * * *").unwrap(),
            run_at: future_time,
        };

        cron.add_job(job).unwrap();
        assert_eq!(cron.queue_len(), 1);
    }

    #[tokio::test]
    async fn once_job_with_past_run_at_is_not_scheduled() {
        let mut cron = Cron::new();
        let past_time = Utc::now() - Duration::seconds(30);

        let job = OnceJob {
            schedule: ValidatedSchedule::parse("* * * * * * *").unwrap(),
            run_at: past_time,
        };

        cron.add_job(job).unwrap();
        // One-time jobs in the past should not be scheduled
        assert_eq!(cron.queue_len(), 0);
    }
}

mod misfire_policy {
    use super::*;

    #[test]
    fn default_misfire_policy_is_skip() {
        struct TestJob {
            schedule: ValidatedSchedule,
        }

        #[async_trait]
        impl JobContract for TestJob {
            async fn run(&self) -> CronResult<()> {
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> {
                Cow::Borrowed("test")
            }
            fn name(&self) -> Cow<'_, str> {
                Cow::Borrowed("Test")
            }
            fn schedule(&self) -> &dyn Schedule {
                &self.schedule
            }
        }

        let job = TestJob {
            schedule: ValidatedSchedule::parse("*/5 * * * * * *").unwrap(),
        };
        assert_eq!(job.misfire_policy(), MisfirePolicy::Skip);
    }

    #[test]
    fn skip_policy_skips_missed_runs() {
        struct SkipJob {
            schedule: ValidatedSchedule,
        }

        #[async_trait]
        impl JobContract for SkipJob {
            async fn run(&self) -> CronResult<()> {
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> {
                Cow::Borrowed("skip")
            }
            fn name(&self) -> Cow<'_, str> {
                Cow::Borrowed("Skip")
            }
            fn schedule(&self) -> &dyn Schedule {
                &self.schedule
            }
            fn misfire_policy(&self) -> MisfirePolicy {
                MisfirePolicy::Skip
            }
        }

        let job = SkipJob {
            schedule: ValidatedSchedule::parse("*/5 * * * * * *").unwrap(),
        };
        assert_eq!(job.misfire_policy(), MisfirePolicy::Skip);
    }

    #[test]
    fn fire_once_policy_executes_once_when_missed() {
        struct FireOnceJob {
            schedule: ValidatedSchedule,
        }

        #[async_trait]
        impl JobContract for FireOnceJob {
            async fn run(&self) -> CronResult<()> {
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> {
                Cow::Borrowed("fire-once")
            }
            fn name(&self) -> Cow<'_, str> {
                Cow::Borrowed("Fire Once")
            }
            fn schedule(&self) -> &dyn Schedule {
                &self.schedule
            }
            fn misfire_policy(&self) -> MisfirePolicy {
                MisfirePolicy::FireOnce
            }
        }

        let job = FireOnceJob {
            schedule: ValidatedSchedule::parse("*/5 * * * * * *").unwrap(),
        };
        assert_eq!(job.misfire_policy(), MisfirePolicy::FireOnce);
    }

    #[test]
    fn fire_all_policy_executes_all_missed_runs() {
        struct FireAllJob {
            schedule: ValidatedSchedule,
        }

        #[async_trait]
        impl JobContract for FireAllJob {
            async fn run(&self) -> CronResult<()> {
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> {
                Cow::Borrowed("fire-all")
            }
            fn name(&self) -> Cow<'_, str> {
                Cow::Borrowed("Fire All")
            }
            fn schedule(&self) -> &dyn Schedule {
                &self.schedule
            }
            fn misfire_policy(&self) -> MisfirePolicy {
                MisfirePolicy::FireAll
            }
        }

        let job = FireAllJob {
            schedule: ValidatedSchedule::parse("*/5 * * * * * *").unwrap(),
        };
        assert_eq!(job.misfire_policy(), MisfirePolicy::FireAll);
    }
}

mod description {
    use super::*;

    #[test]
    fn description_defaults_to_none() {
        struct NoDescriptionJob {
            schedule: ValidatedSchedule,
        }

        #[async_trait]
        impl JobContract for NoDescriptionJob {
            async fn run(&self) -> CronResult<()> {
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> {
                Cow::Borrowed("no-desc")
            }
            fn name(&self) -> Cow<'_, str> {
                Cow::Borrowed("No Description")
            }
            fn schedule(&self) -> &dyn Schedule {
                &self.schedule
            }
        }

        let job = NoDescriptionJob {
            schedule: ValidatedSchedule::parse("*/5 * * * * * *").unwrap(),
        };
        assert!(job.description().is_none());
    }

    #[test]
    fn custom_description_is_returned() {
        struct DescribedJob {
            schedule: ValidatedSchedule,
        }

        #[async_trait]
        impl JobContract for DescribedJob {
            async fn run(&self) -> CronResult<()> {
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> {
                Cow::Borrowed("desc")
            }
            fn name(&self) -> Cow<'_, str> {
                Cow::Borrowed("Described")
            }
            fn schedule(&self) -> &dyn Schedule {
                &self.schedule
            }
            fn description(&self) -> Option<Cow<'_, str>> {
                Some(Cow::Borrowed("This is a test job"))
            }
        }

        let job = DescribedJob {
            schedule: ValidatedSchedule::parse("*/5 * * * * * *").unwrap(),
        };
        assert_eq!(job.description(), Some(Cow::Borrowed("This is a test job")));
    }
}

mod start_after {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn start_after_defaults_to_none() {
        struct DefaultJob {
            schedule: ValidatedSchedule,
        }

        #[async_trait]
        impl JobContract for DefaultJob {
            async fn run(&self) -> CronResult<()> {
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> {
                Cow::Borrowed("default")
            }
            fn name(&self) -> Cow<'_, str> {
                Cow::Borrowed("Default")
            }
            fn schedule(&self) -> &dyn Schedule {
                &self.schedule
            }
        }

        let job = DefaultJob {
            schedule: ValidatedSchedule::parse("*/5 * * * * * *").unwrap(),
        };
        assert!(job.start_after().is_none());
    }

    #[test]
    fn start_after_returns_specified_time() {
        struct DelayedJob {
            schedule: ValidatedSchedule,
            start_time: chrono::DateTime<Utc>,
        }

        #[async_trait]
        impl JobContract for DelayedJob {
            async fn run(&self) -> CronResult<()> {
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> {
                Cow::Borrowed("delayed")
            }
            fn name(&self) -> Cow<'_, str> {
                Cow::Borrowed("Delayed")
            }
            fn schedule(&self) -> &dyn Schedule {
                &self.schedule
            }
            fn start_after(&self) -> Option<chrono::DateTime<Utc>> {
                Some(self.start_time)
            }
        }

        let future_time = Utc::now() + Duration::hours(1);
        let job = DelayedJob {
            schedule: ValidatedSchedule::parse("*/5 * * * * * *").unwrap(),
            start_time: future_time,
        };
        assert_eq!(job.start_after(), Some(future_time));
    }
}
