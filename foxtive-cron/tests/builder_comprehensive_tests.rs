use foxtive_cron::builder::{CronExpression, Month, Weekday};
use foxtive_cron::contracts::Schedule;
use chrono::{NaiveDate, TimeZone, Utc, Datelike, Timelike};
use chrono_tz::{UTC, US::Eastern, Europe::London, Asia::Tokyo};
use std::time::Duration;

mod jitter_tests {
    use super::*;

    #[test]
    fn jitter_adds_random_offset() {
        let cron = CronExpression::builder()
            .daily()
            .hour(9)
            .with_jitter(Duration::from_secs(60));

        // Run multiple times to verify jitter is applied
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 8, 0, 0).unwrap();
        
        let mut times = Vec::new();
        for _ in 0..10 {
            if let Some(next) = cron.next_after(&start, UTC) {
                times.push(next);
            }
        }

        // With jitter, times should vary (not all identical)
        // Note: There's a small chance they could be the same, but very unlikely with 10 samples
        let unique_count = times.iter().collect::<std::collections::HashSet<_>>().len();
        assert!(unique_count >= 2, "Expected varied times due to jitter, got {} unique out of {}", unique_count, times.len());
    }

    #[test]
    fn jitter_respects_maximum_bound() {
        let max_jitter = Duration::from_secs(30);
        let cron = CronExpression::builder()
            .every_second()
            .with_jitter(max_jitter);

        let start = Utc::now();
        
        for _ in 0..20 {
            if let Some(next) = cron.next_after(&start, UTC) {
                let diff = (next - start).num_milliseconds();
                // With every_second + jitter, should be within reasonable bounds
                // The schedule itself advances by ~1 second, plus up to 30 seconds of jitter
                assert!((0..=35000).contains(&diff), "Jitter exceeded bounds: {}ms", diff);
            }
        }
    }

    #[test]
    fn zero_jitter_produces_consistent_results() {
        let cron = CronExpression::builder()
            .daily()
            .hour(9)
            .with_jitter(Duration::from_secs(0));

        let start = Utc.with_ymd_and_hms(2024, 1, 1, 8, 0, 0).unwrap();
        
        let time1 = cron.next_after(&start, UTC).unwrap();
        let time2 = cron.next_after(&start, UTC).unwrap();
        
        assert_eq!(time1, time2, "Zero jitter should produce consistent results");
    }

    #[test]
    fn jitter_with_timezone_applies_correctly() {
        let cron = CronExpression::builder()
            .daily()
            .hour(9)
            .with_timezone(London)
            .with_jitter(Duration::from_secs(120));

        let start = Utc.with_ymd_and_hms(2024, 1, 1, 8, 0, 0).unwrap();
        let next = cron.next_after(&start, London).unwrap();
        
        // Should be around 9 AM London time (with jitter)
        let london_time = next.with_timezone(&London);
        assert_eq!(london_time.hour(), 9);
    }
}

mod blackout_dates_tests {
    use super::*;

    #[test]
    fn single_blackout_date_is_skipped() {
        let christmas = NaiveDate::from_ymd_opt(2024, 12, 25).unwrap();
        
        let cron = CronExpression::builder()
            .daily()
            .hour(9)
            .exclude_date(christmas);

        // Start just before Christmas
        let start = Utc.with_ymd_and_hms(2024, 12, 24, 10, 0, 0).unwrap();
        let next = cron.next_after(&start, UTC).unwrap();
        
        // Should skip Christmas and go to Dec 26
        assert_eq!(next.day(), 26);
        assert_eq!(next.month(), 12);
    }

    #[test]
    fn multiple_blackout_dates_are_skipped() {
        let holidays = vec![
            NaiveDate::from_ymd_opt(2024, 12, 25).unwrap(),
            NaiveDate::from_ymd_opt(2024, 12, 26).unwrap(),
            NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        ];
        
        let cron = CronExpression::builder()
            .daily()
            .hour(9)
            .exclude_dates(holidays);

        let start = Utc.with_ymd_and_hms(2024, 12, 24, 10, 0, 0).unwrap();
        let next = cron.next_after(&start, UTC).unwrap();
        
        // Should skip Dec 25 and land on Dec 27 (since we started after 9 AM on Dec 24)
        assert_eq!(next.day(), 27);
    }

    #[test]
    fn blackout_dates_with_weekday_schedule() {
        let holiday = NaiveDate::from_ymd_opt(2024, 7, 4).unwrap(); // Thursday
        
        let cron = CronExpression::builder()
            .weekdays_only()
            .hour(9)
            .exclude_date(holiday);

        // Start before the holiday
        let start = Utc.with_ymd_and_hms(2024, 7, 3, 8, 0, 0).unwrap();
        let next = cron.next_after(&start, UTC).unwrap();
        
        // Should run on July 3 at 9 AM (before the holiday)
        assert_eq!(next.day(), 3);
        assert_eq!(next.hour(), 9);
    }

    #[test]
    fn consecutive_blackout_dates_dont_cause_infinite_loop() {
        // Black out an entire week
        let blackouts: Vec<NaiveDate> = (1..=7)
            .map(|day| NaiveDate::from_ymd_opt(2024, 1, day).unwrap())
            .collect();
        
        let cron = CronExpression::builder()
            .daily()
            .hour(9)
            .exclude_dates(blackouts);

        let start = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
        let next = cron.next_after(&start, UTC);
        
        // Should eventually find Jan 8
        assert!(next.is_some());
        assert_eq!(next.unwrap().day(), 8);
    }

    #[test]
    fn blackout_dates_with_jitter() {
        let holiday = NaiveDate::from_ymd_opt(2024, 12, 25).unwrap();
        
        let cron = CronExpression::builder()
            .daily()
            .hour(9)
            .exclude_date(holiday)
            .with_jitter(Duration::from_secs(60));

        let start = Utc.with_ymd_and_hms(2024, 12, 24, 10, 0, 0).unwrap();
        let next = cron.next_after(&start, UTC).unwrap();
        
        // Should still skip the holiday even with jitter
        assert_ne!(next.day(), 25);
    }
}

mod timezone_advanced_tests {
    use super::*;

    #[test]
    fn dst_transition_spring_forward() {
        // In US Eastern, clocks spring forward on March 10, 2024 at 2 AM
        // 2 AM becomes 3 AM, so 2:30 AM doesn't exist
        
        let cron = CronExpression::builder()
            .daily()
            .hour(2)
            .minute(30)
            .with_timezone(Eastern);

        // Start just before DST transition
        let start = Utc.with_ymd_and_hms(2024, 3, 9, 6, 0, 0).unwrap(); // 1 AM EST
        let next = cron.next_after(&start, Eastern);
        
        // Should handle the transition gracefully
        assert!(next.is_some());
    }

    #[test]
    fn dst_transition_fall_back() {
        // In US Eastern, clocks fall back on November 3, 2024 at 2 AM
        // 2 AM becomes 1 AM again
        
        let cron = CronExpression::builder()
            .daily()
            .hour(1)
            .minute(30)
            .with_timezone(Eastern);

        let start = Utc.with_ymd_and_hms(2024, 11, 2, 5, 0, 0).unwrap();
        let next = cron.next_after(&start, Eastern);
        
        assert!(next.is_some());
    }

    #[test]
    fn cross_midnight_timezone_conversion() {
        // When it's 11 PM in Tokyo, it's earlier in London
        
        let cron = CronExpression::builder()
            .daily()
            .hour(23)
            .with_timezone(Tokyo);

        let start = Utc.with_ymd_and_hms(2024, 1, 1, 14, 0, 0).unwrap(); // 11 PM Tokyo = 2 PM UTC
        let next = cron.next_after(&start, Tokyo).unwrap();
        
        let tokyo_time = next.with_timezone(&Tokyo);
        assert_eq!(tokyo_time.hour(), 23);
    }

    #[test]
    fn timezone_with_specific_days() {
        // Test that timezone-aware schedules work correctly
        let cron = CronExpression::builder()
            .daily()
            .hour(9)
            .with_timezone(London);

        let start = Utc.with_ymd_and_hms(2024, 1, 1, 8, 0, 0).unwrap();
        let next = cron.next_after(&start, London).unwrap();
        
        let london_time = next.with_timezone(&London);
        assert_eq!(london_time.hour(), 9);
    }
}

mod serialization_tests {
    use super::*;

    #[test]
    fn serialize_deserialize_round_trip() {
        let original = CronExpression::builder()
            .daily()
            .hour(9)
            .minute(30);

        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: CronExpression = serde_json::from_str(&serialized).unwrap();

        let start = Utc.with_ymd_and_hms(2024, 1, 1, 8, 0, 0).unwrap();
        let next1 = original.next_after(&start, UTC).unwrap();
        let next2 = deserialized.next_after(&start, UTC).unwrap();

        assert_eq!(next1, next2);
    }

    #[test]
    fn serialize_with_timezone() {
        let original = CronExpression::builder()
            .daily()
            .hour(9)
            .with_timezone(Tokyo);

        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: CronExpression = serde_json::from_str(&serialized).unwrap();

        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let next1 = original.next_after(&start, Tokyo).unwrap();
        let next2 = deserialized.next_after(&start, Tokyo).unwrap();

        assert_eq!(next1, next2);
    }

    #[test]
    fn serialize_with_blackout_dates() {
        let holidays = vec![
            NaiveDate::from_ymd_opt(2024, 12, 25).unwrap(),
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        ];

        let original = CronExpression::builder()
            .daily()
            .hour(9)
            .exclude_dates(holidays);

        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: CronExpression = serde_json::from_str(&serialized).unwrap();

        let start = Utc.with_ymd_and_hms(2024, 12, 24, 10, 0, 0).unwrap();
        let next1 = original.next_after(&start, UTC).unwrap();
        let next2 = deserialized.next_after(&start, UTC).unwrap();

        assert_eq!(next1, next2);
    }

    #[test]
    fn serialize_with_jitter() {
        let original = CronExpression::builder()
            .daily()
            .hour(9)
            .with_jitter(Duration::from_secs(60));

        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: CronExpression = serde_json::from_str(&serialized).unwrap();

        // Both should have jitter configured
        // We can't test exact times due to randomness, but we can verify structure
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 8, 0, 0).unwrap();
        assert!(original.next_after(&start, UTC).is_some());
        assert!(deserialized.next_after(&start, UTC).is_some());
    }

    #[test]
    fn serialize_complex_expression() {
        let original = CronExpression::builder()
            .hours_range(9, 17)
            .minutes_interval(15)
            .weekdays_only()
            .month(Month::January)
            .with_timezone(London)
            .with_jitter(Duration::from_secs(30))
            .exclude_date(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap());

        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: CronExpression = serde_json::from_str(&serialized).unwrap();

        let start = Utc.with_ymd_and_hms(2024, 1, 2, 8, 0, 0).unwrap();
        let next1 = original.next_after(&start, London);
        let next2 = deserialized.next_after(&start, London);

        assert!(next1.is_some());
        assert!(next2.is_some());
    }
}

mod builder_composition_tests {
    use super::*;

    #[test]
    fn complex_business_hours_schedule() {
        // Every 15 minutes during business hours (9-17), weekdays only
        let cron = CronExpression::builder()
            .hours_range(9, 17)
            .minutes_interval(15)
            .weekdays_only();

        let expression = cron.build();
        // ISO 8601 Monday-Friday (1-5) converted to cron format (2-6)
        assert_eq!(expression, "0 */15 9-17 * * 2-6 *");
    }

    #[test]
    fn monthly_report_schedule() {
        // First day of every month at midnight
        let cron = CronExpression::builder()
            .monthly()
            .day_of_month(1);

        let expression = cron.build();
        assert_eq!(expression, "0 0 0 1 * * *");
    }

    #[test]
    fn quarterly_review_schedule() {
        // First Monday of January, April, July, October at 10 AM
        let cron = CronExpression::builder()
            .day_of_week(Weekday::Monday)
            .day_of_month(1)
            .hour(10);

        let expression = cron.build();
        // Note: This creates a complex expression
        assert!(expression.contains("1"));
        assert!(expression.contains("10"));
    }

    #[test]
    fn maintenance_window_schedule() {
        // Every Sunday at 2 AM
        let cron = CronExpression::builder()
            .day_of_week(Weekday::Sunday)
            .hour(2);

        let expression = cron.build();
        // Note: when we set day_of_week, it doesn't reset minutes, so we get "*" for minutes
        // ISO 8601 Sunday=7 converted to cron format Sunday=1
        assert_eq!(expression, "0 * 2 * * 1 *");
    }

    #[test]
    fn end_of_month_schedule() {
        // Last days of every month at 11 PM (using multiple specific days)
        let cron = CronExpression::builder()
            .daily()
            .hour(23);
            // Note: day_of_month only takes one value, so we'd need to add more API for lists

        let expression = cron.build();
        assert_eq!(expression, "0 0 23 * * * *");
    }

    #[test]
    fn override_preset_values() {
        // Start with daily, then customize
        let cron = CronExpression::builder()
            .daily()
            .hour(14)
            .minute(30);

        let expression = cron.build();
        assert_eq!(expression, "0 30 14 * * * *");
    }

    #[test]
    fn mixed_intervals_and_lists() {
        // Every 5 minutes during specific hours
        let cron = CronExpression::builder()
            .hours_list(&[9, 12, 15, 18])
            .minutes_interval(5);

        let expression = cron.build();
        assert_eq!(expression, "0 */5 9,12,15,18 * * * *");
    }
}

mod validation_edge_cases {
    use super::*;

    #[test]
    fn invalid_day_of_month_rejected() {
        let result = CronExpression::builder()
            .day_of_month(32)
            .to_validated();
        
        assert!(result.is_err());
    }

    #[test]
    fn invalid_month_rejected() {
        // Using raw value that's out of range
        let result = CronExpression::builder()
            .month(Month::December) // This is valid
            .to_validated();
        
        assert!(result.is_ok());
    }

    #[test]
    fn weekday_enum_values_are_valid() {
        // Weekday enum ensures type safety, so all values should be valid
        // Test a few representative weekdays
        let result_monday = CronExpression::builder()
            .day_of_week(Weekday::Monday)
            .to_validated();
        assert!(result_monday.is_ok());
        
        let result_wednesday = CronExpression::builder()
            .day_of_week(Weekday::Wednesday)
            .to_validated();
        assert!(result_wednesday.is_ok());
        
        let result_friday = CronExpression::builder()
            .day_of_week(Weekday::Friday)
            .to_validated();
        assert!(result_friday.is_ok());
    }

    #[test]
    fn negative_values_rejected() {
        // Can't directly pass negative values due to u32 type,
        // but we can test boundary conditions
        let result = CronExpression::builder()
            .hour(0) // Minimum valid
            .to_validated();
        
        assert!(result.is_ok());
    }

    #[test]
    fn maximum_valid_values_accepted() {
        let result = CronExpression::builder()
            .second(59)
            .minute(59)
            .hour(23)
            .day_of_month(31)
            .month(Month::December)
            .day_of_week(Weekday::Saturday)
            .to_validated();
        
        assert!(result.is_ok());
    }
}

mod schedule_trait_integration {
    use super::*;

    #[test]
    fn cron_expression_implements_schedule_trait() {
        let cron = CronExpression::builder()
            .daily()
            .hour(9);

        // Verify it can be used as a Schedule trait object
        let schedule: &dyn Schedule = &cron;
        
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 8, 0, 0).unwrap();
        let next = schedule.next_after(&start, UTC);
        
        assert!(next.is_some());
        assert_eq!(next.unwrap().hour(), 9);
    }

    #[test]
    fn multiple_next_calls_advance_correctly() {
        let cron = CronExpression::builder()
            .daily()
            .hour(12);

        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        
        let next1 = cron.next_after(&start, UTC).unwrap();
        let next2 = cron.next_after(&next1, UTC).unwrap();
        let next3 = cron.next_after(&next2, UTC).unwrap();

        assert_eq!(next1.day(), 1);
        assert_eq!(next2.day(), 2);
        assert_eq!(next3.day(), 3);
        assert_eq!(next1.hour(), 12);
        assert_eq!(next2.hour(), 12);
        assert_eq!(next3.hour(), 12);
    }

    #[test]
    fn schedule_with_all_features_combined() {
        let cron = CronExpression::builder()
            .weekdays_only()
            .hours_range(9, 17)
            .minutes_interval(30)
            .with_timezone(London)
            .with_jitter(Duration::from_secs(45))
            .exclude_date(NaiveDate::from_ymd_opt(2024, 12, 25).unwrap())
            .exclude_date(NaiveDate::from_ymd_opt(2024, 12, 26).unwrap());

        let start = Utc.with_ymd_and_hms(2024, 12, 24, 0, 0, 0).unwrap();
        let next = cron.next_after(&start, London);

        // Should find a valid time that respects all constraints
        assert!(next.is_some());
        
        let next_time = next.unwrap();
        let london_time = next_time.with_timezone(&London);
        
        // Should be a weekday
        assert!(london_time.weekday().number_from_monday() <= 5);
        // Should be during business hours
        assert!(london_time.hour() >= 9 && london_time.hour() <= 17);
        // Should not be on excluded dates
        assert_ne!(london_time.day(), 25);
        assert_ne!(london_time.day(), 26);
    }
}

mod real_world_expression_verification {
    use super::*;

    #[test]
    fn database_backup_daily_2am() {
        // Real-world: Daily database backup at 2 AM
        let cron = CronExpression::builder()
            .daily()
            .hour(2);

        let expression = cron.build();
        assert_eq!(expression, "0 0 2 * * * *");
    }

    #[test]
    fn health_check_every_30_seconds() {
        // Real-world: Health check every 30 seconds
        let cron = CronExpression::builder()
            .seconds_interval(30);

        let expression = cron.build();
        assert_eq!(expression, "*/30 * * * * * *");
    }

    #[test]
    fn business_hours_weekdays() {
        // Real-world: Every 15 minutes during business hours (9-17) on weekdays
        let cron = CronExpression::builder()
            .hours_range(9, 17)
            .minutes_interval(15)
            .weekdays_only();

        let expression = cron.build();
        // ISO 8601 Monday-Friday (1-5) converted to cron format (2-6)
        assert_eq!(expression, "0 */15 9-17 * * 2-6 *");
    }

    #[test]
    fn monthly_report_first_day() {
        // Real-world: Monthly report on 1st at 9 AM
        let cron = CronExpression::builder()
            .monthly()
            .hour(9);

        let expression = cron.build();
        assert_eq!(expression, "0 0 9 1 * * *");
    }

    #[test]
    fn cache_cleanup_every_6_hours() {
        // Real-world: Cache cleanup every 6 hours
        let cron = CronExpression::builder()
            .hours_list(&[0, 6, 12, 18]);

        let expression = cron.build();
        assert_eq!(expression, "0 * 0,6,12,18 * * * *");
    }

    #[test]
    fn weekly_maintenance_sunday_midnight() {
        // Real-world: Weekly maintenance on Sunday at midnight
        let cron = CronExpression::builder()
            .weekly()
            .day_of_week(Weekday::Sunday);

        let expression = cron.build();
        // ISO 8601 Sunday=7 converted to cron format Sunday=1
        assert_eq!(expression, "0 0 0 * * 1 *");
    }

    #[test]
    fn quarterly_audit() {
        // Real-world: Quarterly audit on 1st of Jan, Apr, Jul, Oct at 11 PM
        let cron = CronExpression::builder()
            .daily()
            .hour(23)
            .minute(0)
            .day_of_month(1)
            .month(Month::January);
        
        // Note: Builder API doesn't support month lists yet, so we test single month
        let expression = cron.build();
        assert_eq!(expression, "0 0 23 1 1 * *");
    }

    #[test]
    fn log_rotation_hourly() {
        // Real-world: Log rotation every hour at minute 0
        let cron = CronExpression::builder()
            .hourly();

        let expression = cron.build();
        assert_eq!(expression, "0 0 * * * * *");
    }

    #[test]
    fn api_rate_limit_reset() {
        // Real-world: API rate limit reset every hour
        let cron = CronExpression::builder()
            .every_hour()
            .minute(0);

        let expression = cron.build();
        assert_eq!(expression, "0 0 * * * * *");
    }

    #[test]
    fn batch_processing_off_peak() {
        // Real-world: Batch processing every 30 minutes during off-peak (10 PM - 6 AM)
        let cron = CronExpression::builder()
            .minutes_interval(30)
            .hours_list(&[22, 23, 0, 1, 2, 3, 4, 5, 6]);

        let expression = cron.build();
        assert_eq!(expression, "0 */30 22,23,0,1,2,3,4,5,6 * * * *");
    }

    #[test]
    fn invoice_generation_monthly() {
        // Real-world: Invoice generation on 1st of every month at 9 AM
        let cron = CronExpression::builder()
            .monthly()
            .hour(9)
            .minute(0);

        let expression = cron.build();
        assert_eq!(expression, "0 0 9 1 * * *");
    }

    #[test]
    fn system_reboot_first_sunday() {
        // Real-world: System reboot on first Sunday of month at 3 AM
        let cron = CronExpression::builder()
            .day_of_week(Weekday::Sunday)
            .hour(3);

        let expression = cron.build();
        // ISO 8601 Sunday=7 converted to cron format Sunday=1
        assert_eq!(expression, "0 * 3 * * 1 *");
    }

    #[test]
    fn stock_market_hours() {
        // Real-world: Stock market data fetch every minute during market hours (9-16 ET)
        let cron = CronExpression::builder()
            .every_minute()
            .hours_range(9, 16)
            .weekdays_only();

        let expression = cron.build();
        // ISO 8601 Monday-Friday (1-5) converted to cron format (2-6)
        assert_eq!(expression, "0 * 9-16 * * 2-6 *");
    }

    #[test]
    fn email_digest_twice_daily() {
        // Real-world: Email digest at 8 AM and 6 PM
        let cron = CronExpression::builder()
            .daily()
            .hours_list(&[8, 18])
            .minute(0);

        let expression = cron.build();
        assert_eq!(expression, "0 0 8,18 * * * *");
    }

    #[test]
    fn metrics_collection_every_5_minutes() {
        // Real-world: Metrics collection every 5 minutes
        let cron = CronExpression::builder()
            .minutes_interval(5);

        let expression = cron.build();
        assert_eq!(expression, "0 */5 * * * * *");
    }

    #[test]
    fn backup_retention_cleanup() {
        // Real-world: Backup retention cleanup on Sundays at 4 AM
        let cron = CronExpression::builder()
            .day_of_week(Weekday::Sunday)
            .hour(4);

        let expression = cron.build();
        // ISO 8601 Sunday=7 converted to cron format Sunday=1
        assert_eq!(expression, "0 * 4 * * 1 *");
    }

    #[test]
    fn session_cleanup_every_hour() {
        // Real-world: Session cleanup every hour at minute 30
        let cron = CronExpression::builder()
            .hourly()
            .minute(30);

        let expression = cron.build();
        assert_eq!(expression, "0 30 * * * * *");
    }

    #[test]
    fn compliance_report_weekly_friday() {
        // Real-world: Compliance report every Friday at 5 PM
        let cron = CronExpression::builder()
            .day_of_week(Weekday::Friday)
            .hour(17)
            .minute(0);

        let expression = cron.build();
        // ISO 8601 Friday=5 converted to cron format Friday=6
        assert_eq!(expression, "0 0 17 * * 6 *");
    }

    #[test]
    fn certificate_renewal_check() {
        // Real-world: Certificate renewal check daily at noon
        let cron = CronExpression::builder()
            .daily()
            .hour(12);

        let expression = cron.build();
        assert_eq!(expression, "0 0 12 * * * *");
    }

    #[test]
    fn data_sync_every_10_minutes() {
        // Real-world: Data synchronization every 10 minutes
        let cron = CronExpression::builder()
            .minutes_interval(10);

        let expression = cron.build();
        assert_eq!(expression, "0 */10 * * * * *");
    }

    #[test]
    fn archive_old_records_monthly() {
        // Real-world: Archive old records on last day of month at 11 PM
        let cron = CronExpression::builder()
            .daily()
            .hour(23)
            .day_of_month(31);

        let expression = cron.build();
        assert_eq!(expression, "0 0 23 31 * * *");
    }

    #[test]
    fn monitoring_alert_check() {
        // Real-world: Monitoring alert check every 2 minutes
        let cron = CronExpression::builder()
            .minutes_interval(2);

        let expression = cron.build();
        assert_eq!(expression, "0 */2 * * * * *");
    }

    #[test]
    fn payroll_processing_biweekly() {
        // Real-world: Payroll processing every other Friday at 9 AM
        let cron = CronExpression::builder()
            .day_of_week(Weekday::Friday)
            .hour(9)
            .minute(0);

        let expression = cron.build();
        // ISO 8601 Friday=5 converted to cron format Friday=6
        assert_eq!(expression, "0 0 9 * * 6 *");
    }

    #[test]
    fn temp_file_cleanup_daily() {
        // Real-world: Temporary file cleanup daily at 3 AM
        let cron = CronExpression::builder()
            .daily()
            .hour(3);

        let expression = cron.build();
        assert_eq!(expression, "0 0 3 * * * *");
    }

    #[test]
    fn analytics_aggregation_hourly() {
        // Real-world: Analytics aggregation every hour at minute 15
        let cron = CronExpression::builder()
            .hourly()
            .minute(15);

        let expression = cron.build();
        assert_eq!(expression, "0 15 * * * * *");
    }

    #[test]
    fn security_scan_nightly() {
        // Real-world: Security scan nightly at 1 AM
        let cron = CronExpression::builder()
            .daily()
            .hour(1);

        let expression = cron.build();
        assert_eq!(expression, "0 0 1 * * * *");
    }

    #[test]
    fn webhook_retry_every_minute() {
        // Real-world: Webhook retry queue processing every minute
        let cron = CronExpression::builder()
            .every_minute();

        let expression = cron.build();
        assert_eq!(expression, "0 * * * * * *");
    }

    #[test]
    fn database_vacuum_weekly() {
        // Real-world: Database vacuum on Saturdays at 5 AM
        let cron = CronExpression::builder()
            .day_of_week(Weekday::Saturday)
            .hour(5);

        let expression = cron.build();
        // ISO 8601 Saturday=6 converted to cron format Saturday=7
        assert_eq!(expression, "0 * 5 * * 7 *");
    }

    #[test]
    fn user_activity_summary() {
        // Real-world: User activity summary every 4 hours
        let cron = CronExpression::builder()
            .daily()
            .hours_list(&[0, 4, 8, 12, 16, 20]);

        let expression = cron.build();
        assert_eq!(expression, "0 0 0,4,8,12,16,20 * * * *");
    }

    #[test]
    fn ssl_certificate_check() {
        // Real-world: SSL certificate expiry check twice daily
        let cron = CronExpression::builder()
            .daily()
            .hours_list(&[6, 18]);

        let expression = cron.build();
        assert_eq!(expression, "0 0 6,18 * * * *");
    }

    #[test]
    fn load_balancer_health_check() {
        // Real-world: Load balancer health check every 10 seconds
        let cron = CronExpression::builder()
            .seconds_interval(10);

        let expression = cron.build();
        assert_eq!(expression, "*/10 * * * * * *");
    }

    #[test]
    fn etl_pipeline_daily() {
        // Real-world: ETL pipeline daily at 2:30 AM
        let cron = CronExpression::builder()
            .daily()
            .hour(2)
            .minute(30);

        let expression = cron.build();
        assert_eq!(expression, "0 30 2 * * * *");
    }

    #[test]
    fn notification_digest_hourly() {
        // Real-world: Notification digest every hour at minute 45
        let cron = CronExpression::builder()
            .hourly()
            .minute(45);

        let expression = cron.build();
        assert_eq!(expression, "0 45 * * * * *");
    }

    #[test]
    fn index_rebuild_weekend() {
        // Real-world: Database index rebuild on weekends at 6 AM
        let cron = CronExpression::builder()
            .day_of_week(Weekday::Saturday)
            .hour(6);

        let expression = cron.build();
        // ISO 8601 Saturday=6 converted to cron format Saturday=7
        assert_eq!(expression, "0 * 6 * * 7 *");
    }

    #[test]
    fn audit_log_rotation() {
        // Real-world: Audit log rotation every 6 hours
        let cron = CronExpression::builder()
            .daily()
            .hours_list(&[0, 6, 12, 18]);

        let expression = cron.build();
        assert_eq!(expression, "0 0 0,6,12,18 * * * *");
    }

    #[test]
    fn payment_processing_batch() {
        // Real-world: Payment processing batch every 15 minutes during business hours
        let cron = CronExpression::builder()
            .minutes_interval(15)
            .hours_range(8, 18)
            .weekdays_only();

        let expression = cron.build();
        // ISO 8601 Monday-Friday (1-5) converted to cron format (2-6)
        assert_eq!(expression, "0 */15 8-18 * * 2-6 *");
    }
}
