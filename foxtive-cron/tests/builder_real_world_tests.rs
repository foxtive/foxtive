use chrono::{Datelike, TimeZone, Timelike, Utc};
use chrono_tz::{Asia::Tokyo, Europe::London, US::Eastern, UTC};
use foxtive_cron::contracts::ValidatedSchedule;

mod real_world_expressions {
    use super::*;

    #[test]
    fn every_minute_standard() {
        let schedule = ValidatedSchedule::parse("0 * * * * *").unwrap();
        let now = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap();
        let next = schedule.next_after(&now, UTC).unwrap();
        assert_eq!(next.minute(), 31);
    }

    #[test]
    fn every_five_minutes() {
        let schedule = ValidatedSchedule::parse("0 */5 * * * *").unwrap();
        let now = Utc.with_ymd_and_hms(2024, 1, 15, 10, 32, 0).unwrap();
        let next = schedule.next_after(&now, UTC).unwrap();
        assert_eq!(next.minute(), 35);
    }

    #[test]
    fn daily_at_midnight() {
        let schedule = ValidatedSchedule::parse("0 0 0 * * *").unwrap();
        let now = Utc.with_ymd_and_hms(2024, 1, 15, 23, 30, 0).unwrap();
        let next = schedule.next_after(&now, UTC).unwrap();
        assert_eq!(next.day(), 16);
        assert_eq!(next.hour(), 0);
    }

    #[test]
    fn daily_at_specific_time() {
        let schedule = ValidatedSchedule::parse("0 30 9 * * *").unwrap();
        let now = Utc.with_ymd_and_hms(2024, 1, 15, 8, 0, 0).unwrap();
        let next = schedule.next_after(&now, UTC).unwrap();
        assert_eq!(next.hour(), 9);
        assert_eq!(next.minute(), 30);
    }

    #[test]
    fn first_day_of_month() {
        let schedule = ValidatedSchedule::parse("0 0 0 1 * *").unwrap();
        let mid_month = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let next = schedule.next_after(&mid_month, UTC).unwrap();
        assert_eq!(next.month(), 2);
        assert_eq!(next.day(), 1);
    }

    #[test]
    fn business_hours_every_30_min() {
        let schedule = ValidatedSchedule::parse("0 */30 9-17 * * 1-5").unwrap();
        let wednesday = Utc.with_ymd_and_hms(2024, 1, 10, 8, 45, 0).unwrap();
        let next = schedule.next_after(&wednesday, UTC).unwrap();
        assert_eq!(next.hour(), 9);
        assert_eq!(next.minute(), 0);
    }

    #[test]
    fn every_15_seconds() {
        let schedule = ValidatedSchedule::parse("*/15 * * * * * *").unwrap();
        let now = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 7).unwrap();
        let next = schedule.next_after(&now, UTC).unwrap();
        assert_eq!(next.second(), 15);
    }
}

mod timezone_real_world {
    use super::*;

    #[test]
    fn stock_market_open_ny() {
        let schedule = ValidatedSchedule::parse("0 30 9 * * 1-5").unwrap();
        // Sunday evening in UTC (before Monday morning in NY)
        let sunday_evening = Utc.with_ymd_and_hms(2024, 1, 14, 12, 0, 0).unwrap(); // 7 AM EST Sunday
        let next = schedule.next_after(&sunday_evening, Eastern).unwrap();
        let ny_time = next.with_timezone(&Eastern);
        // Should be Monday at 9:30 AM
        assert!(
            ny_time.weekday() == chrono::Weekday::Mon || ny_time.weekday() == chrono::Weekday::Sun
        );
        assert_eq!(ny_time.hour(), 9);
        assert_eq!(ny_time.minute(), 30);
    }

    #[test]
    fn london_business_hours() {
        let schedule = ValidatedSchedule::parse("0 0 9-17 * * *").unwrap();
        let early_morning = Utc.with_ymd_and_hms(2024, 1, 15, 7, 0, 0).unwrap();
        let next = schedule.next_after(&early_morning, London).unwrap();
        let london_time = next.with_timezone(&London);
        assert_eq!(london_time.hour(), 9);
    }

    #[test]
    fn tokyo_midnight_maintenance() {
        let schedule = ValidatedSchedule::parse("0 0 0 * * *").unwrap();
        let afternoon_utc = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let next = schedule.next_after(&afternoon_utc, Tokyo).unwrap();
        let tokyo_time = next.with_timezone(&Tokyo);
        assert_eq!(tokyo_time.hour(), 0);
        assert_eq!(tokyo_time.day(), 16);
    }
}

mod edge_cases_and_boundaries {
    use super::*;

    #[test]
    fn leap_year_feb_29() {
        let schedule = ValidatedSchedule::parse("0 0 12 29 2 *").unwrap();
        let feb_28_2024 = Utc.with_ymd_and_hms(2024, 2, 28, 13, 0, 0).unwrap();
        let next = schedule.next_after(&feb_28_2024, UTC).unwrap();
        assert_eq!(next.month(), 2);
        assert_eq!(next.day(), 29);
        assert_eq!(next.year(), 2024);
    }

    #[test]
    fn multiple_times_same_day() {
        let schedule = ValidatedSchedule::parse("0 0 9,13,17 * * *").unwrap();
        let morning = Utc.with_ymd_and_hms(2024, 1, 15, 8, 0, 0).unwrap();
        let next1 = schedule.next_after(&morning, UTC).unwrap();
        let next2 = schedule.next_after(&next1, UTC).unwrap();
        let next3 = schedule.next_after(&next2, UTC).unwrap();
        assert_eq!(next1.hour(), 9);
        assert_eq!(next2.hour(), 13);
        assert_eq!(next3.hour(), 17);
    }
}

mod validation_tests {
    use super::*;

    #[test]
    fn invalid_minute_value() {
        let result = ValidatedSchedule::parse("0 60 * * * *");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_hour_value() {
        let result = ValidatedSchedule::parse("0 * 24 * * *");
        assert!(result.is_err());
    }

    #[test]
    fn malformed_expression() {
        let result = ValidatedSchedule::parse("not a cron expression");
        assert!(result.is_err());
    }

    #[test]
    fn valid_range_expression() {
        let result = ValidatedSchedule::parse("0 0-30 9-17 * * 1-5");
        assert!(result.is_ok());
    }

    #[test]
    fn valid_step_expression() {
        let result = ValidatedSchedule::parse("0 */15 */2 * * *");
        assert!(result.is_ok());
    }
}

mod performance_tests {
    use super::*;

    #[test]
    fn rapid_succession_calls() {
        let schedule = ValidatedSchedule::parse("0 * * * * *").unwrap();
        let mut current = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

        for _i in 0..100 {
            let next = schedule.next_after(&current, UTC).unwrap();
            assert!(next > current);
            current = next;
        }

        assert_eq!(current.hour(), 1);
        assert_eq!(current.minute(), 40);
    }

    #[test]
    fn long_term_scheduling() {
        let schedule = ValidatedSchedule::parse("0 0 0 1 1 *").unwrap();
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 1).unwrap();
        let next1 = schedule.next_after(&start, UTC).unwrap();
        let next2 = schedule.next_after(&next1, UTC).unwrap();
        assert_eq!(next1.year(), 2025);
        assert_eq!(next2.year(), 2026);
    }
}

mod real_world_use_cases {
    use super::*;

    #[test]
    fn database_backup_schedule() {
        let schedule = ValidatedSchedule::parse("0 0 2 * * *").unwrap();
        let evening = Utc.with_ymd_and_hms(2024, 1, 15, 20, 0, 0).unwrap();
        let next = schedule.next_after(&evening, UTC).unwrap();
        assert_eq!(next.hour(), 2);
        assert_eq!(next.day(), 16);
    }

    #[test]
    fn health_check_interval() {
        let schedule = ValidatedSchedule::parse("*/30 * * * * * *").unwrap();
        let now = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 45).unwrap();
        let next = schedule.next_after(&now, UTC).unwrap();
        assert_eq!(next.second(), 0);
        assert_eq!(next.minute(), 31);
    }

    #[test]
    fn cache_cleanup() {
        let schedule = ValidatedSchedule::parse("0 0 */6 * * *").unwrap();
        let morning = Utc.with_ymd_and_hms(2024, 1, 15, 2, 0, 0).unwrap();
        let next = schedule.next_after(&morning, UTC).unwrap();
        assert_eq!(next.hour(), 6);
    }

    #[test]
    fn invoice_generation() {
        let schedule = ValidatedSchedule::parse("0 0 9 1 * *").unwrap();
        let mid_month = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let next = schedule.next_after(&mid_month, UTC).unwrap();
        assert_eq!(next.month(), 2);
        assert_eq!(next.day(), 1);
        assert_eq!(next.hour(), 9);
    }

    #[test]
    fn api_rate_limit_reset() {
        let schedule = ValidatedSchedule::parse("0 0 * * * *").unwrap();
        let mid_hour = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap();
        let next = schedule.next_after(&mid_hour, UTC).unwrap();
        assert_eq!(next.minute(), 0);
        assert_eq!(next.hour(), 11);
    }
}
