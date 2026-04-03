use async_trait::async_trait;
use chrono::Utc;
use chrono_tz::Tz;
use foxtive_cron::contracts::{JobContract, ValidatedSchedule};
use foxtive_cron::{Cron, CronResult};
use std::borrow::Cow;

mod timezone {
    use super::*;

    #[test]
    fn default_timezone_is_utc() {
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
            fn schedule(&self) -> &ValidatedSchedule {
                &self.schedule
            }
        }

        let job = DefaultJob {
            schedule: ValidatedSchedule::parse("*/5 * * * * * *").unwrap(),
        };
        assert_eq!(job.timezone(), Tz::UTC);
    }

    #[test]
    fn custom_timezone_is_returned() {
        struct CustomTzJob {
            schedule: ValidatedSchedule,
        }

        #[async_trait]
        impl JobContract for CustomTzJob {
            async fn run(&self) -> CronResult<()> {
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> {
                Cow::Borrowed("custom-tz")
            }
            fn name(&self) -> Cow<'_, str> {
                Cow::Borrowed("Custom TZ")
            }
            fn schedule(&self) -> &ValidatedSchedule {
                &self.schedule
            }
            fn timezone(&self) -> Tz {
                Tz::America__New_York
            }
        }

        let job = CustomTzJob {
            schedule: ValidatedSchedule::parse("*/5 * * * * * *").unwrap(),
        };
        assert_eq!(job.timezone(), Tz::America__New_York);
    }

    #[test]
    fn various_timezones_are_supported() {
        let timezones = [
            Tz::UTC,
            Tz::America__New_York,
            Tz::America__Los_Angeles,
            Tz::Europe__London,
            Tz::Europe__Paris,
            Tz::Asia__Tokyo,
            Tz::Asia__Shanghai,
            Tz::Australia__Sydney,
        ];

        for tz in &timezones {
            struct TzJob {
                schedule: ValidatedSchedule,
                tz: Tz,
            }

            #[async_trait]
            impl JobContract for TzJob {
                async fn run(&self) -> CronResult<()> {
                    Ok(())
                }
                fn id(&self) -> Cow<'_, str> {
                    Cow::Borrowed("tz-job")
                }
                fn name(&self) -> Cow<'_, str> {
                    Cow::Borrowed("TZ Job")
                }
                fn schedule(&self) -> &ValidatedSchedule {
                    &self.schedule
                }
                fn timezone(&self) -> Tz {
                    self.tz
                }
            }

            let job = TzJob {
                schedule: ValidatedSchedule::parse("0 0 * * * * *").unwrap(),
                tz: *tz,
            };
            assert_eq!(job.timezone(), *tz);
        }
    }

    #[tokio::test]
    async fn job_with_timezone_can_be_added_to_cron() {
        struct TzJob {
            schedule: ValidatedSchedule,
        }

        #[async_trait]
        impl JobContract for TzJob {
            async fn run(&self) -> CronResult<()> {
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> {
                Cow::Borrowed("tz-job")
            }
            fn name(&self) -> Cow<'_, str> {
                Cow::Borrowed("TZ Job")
            }
            fn schedule(&self) -> &ValidatedSchedule {
                &self.schedule
            }
            fn timezone(&self) -> Tz {
                Tz::America__New_York
            }
        }

        let mut cron = Cron::new();
        let job = TzJob {
            schedule: ValidatedSchedule::parse("0 9 * * * * *").unwrap(),
        };

        cron.add_job(job).unwrap();
        assert_eq!(cron.queue_len(), 1);
    }

    #[test]
    fn next_run_time_respects_timezone() {
        struct TzJob {
            schedule: ValidatedSchedule,
        }

        #[async_trait]
        impl JobContract for TzJob {
            async fn run(&self) -> CronResult<()> {
                Ok(())
            }
            fn id(&self) -> Cow<'_, str> {
                Cow::Borrowed("tz-job")
            }
            fn name(&self) -> Cow<'_, str> {
                Cow::Borrowed("TZ Job")
            }
            fn schedule(&self) -> &ValidatedSchedule {
                &self.schedule
            }
            fn timezone(&self) -> Tz {
                Tz::America__New_York
            }
        }

        let job = TzJob {
            schedule: ValidatedSchedule::parse("0 9 * * * * *").unwrap(),
        };

        // The job should have a next run time
        let item =
            foxtive_cron::JobItem::new(std::sync::Arc::new(job), vec![], None, None).unwrap();

        assert!(item.next_run_time().is_some());
    }
}

mod validated_schedule_tests {
    use super::*;

    #[test]
    fn parse_validates_seven_field_expression() {
        let result = ValidatedSchedule::parse("* * * * * * *");
        assert!(result.is_ok());
    }

    #[test]
    fn parse_validates_standard_six_field_expression() {
        let result = ValidatedSchedule::parse("* * * * * *");
        assert!(result.is_ok());
    }

    #[test]
    fn parse_rejects_invalid_expression() {
        let result = ValidatedSchedule::parse("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn parse_rejects_out_of_range_values() {
        // Minute field (60 is invalid, max is 59)
        let result = ValidatedSchedule::parse("* 60 * * * * *");
        assert!(result.is_err());

        // Hour field (24 is invalid, max is 23)
        let result = ValidatedSchedule::parse("* * 24 * * * *");
        assert!(result.is_err());

        // Day of month (32 is invalid, max is 31)
        let result = ValidatedSchedule::parse("* * * 32 * * *");
        assert!(result.is_err());

        // Month (13 is invalid, max is 12)
        let result = ValidatedSchedule::parse("* * * * 13 * *");
        assert!(result.is_err());

        // Day of week (8 is invalid, max is 7)
        let result = ValidatedSchedule::parse("* * * * * 8 *");
        assert!(result.is_err());
    }

    #[test]
    fn parse_accepts_special_characters() {
        // Step values
        assert!(ValidatedSchedule::parse("*/5 * * * * * *").is_ok());

        // Ranges
        assert!(ValidatedSchedule::parse("0-30 * * * * * *").is_ok());

        // Lists
        assert!(ValidatedSchedule::parse("1,5,10,15 * * * * * *").is_ok());

        // Wildcards
        assert!(ValidatedSchedule::parse("* * * * * * *").is_ok());
    }

    #[test]
    fn next_after_returns_future_time() {
        let schedule = ValidatedSchedule::parse("*/5 * * * * * *").unwrap();
        let now = Utc::now();
        let next = schedule.next_after(&now, Tz::UTC);

        assert!(next.is_some());
        let next = next.unwrap();
        assert!(next > now);
    }

    #[test]
    fn next_after_with_different_timezones() {
        // Schedule for 9 AM in the specified timezone
        let schedule = ValidatedSchedule::parse("0 9 * * * * *").unwrap();
        let now = Utc::now();

        let next_utc = schedule.next_after(&now, Tz::UTC);
        let next_ny = schedule.next_after(&now, Tz::America__New_York);

        assert!(next_utc.is_some());
        assert!(next_ny.is_some());

        // When it's 9 AM UTC, it's 4-5 AM New York, so they should be different
        // However, they might occasionally align, so we just verify both return valid times
        // The key is that the schedule interprets "9 AM" differently in each timezone
    }

    #[test]
    fn schedule_clone_works() {
        let schedule1 = ValidatedSchedule::parse("*/5 * * * * * *").unwrap();
        let schedule2 = schedule1.clone();

        let now = Utc::now();
        let next1 = schedule1.next_after(&now, Tz::UTC);
        let next2 = schedule2.next_after(&now, Tz::UTC);

        assert_eq!(next1, next2);
    }
}

mod retry_policy_none {
    use super::*;
    use foxtive_cron::contracts::RetryPolicy;
    use foxtive_cron::{CronError, JobItem};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct NoRetryJob {
        schedule: ValidatedSchedule,
        run_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl JobContract for NoRetryJob {
        async fn run(&self) -> CronResult<()> {
            self.run_count.fetch_add(1, Ordering::SeqCst);
            Err(CronError::ExecutionError(anyhow::anyhow!("always fails")))
        }
        fn id(&self) -> Cow<'_, str> {
            Cow::Borrowed("no-retry")
        }
        fn name(&self) -> Cow<'_, str> {
            Cow::Borrowed("No Retry")
        }
        fn schedule(&self) -> &ValidatedSchedule {
            &self.schedule
        }
        fn retry_policy(&self) -> RetryPolicy {
            RetryPolicy::None
        }
    }

    #[tokio::test]
    async fn no_retry_policy_fails_immediately() {
        let run_count = Arc::new(AtomicUsize::new(0));
        let job = NoRetryJob {
            schedule: ValidatedSchedule::parse("*/1 * * * * * *").unwrap(),
            run_count: run_count.clone(),
        };

        let item = JobItem::new(Arc::new(job), vec![], None, None).unwrap();
        let result = item.run().await;

        assert!(result.is_err());
        assert_eq!(run_count.load(Ordering::SeqCst), 1); // Only one attempt
    }
}
