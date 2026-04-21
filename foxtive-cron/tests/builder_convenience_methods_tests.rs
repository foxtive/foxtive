use foxtive_cron::builder::{CronExpression, Month};
use foxtive_cron::contracts::Schedule;
use chrono::{TimeZone, Utc, Datelike};
use chrono_tz::UTC;

mod weekday_convenience_methods {
    use chrono::Timelike;
    use super::*;

    #[test]
    fn sundays_only_produces_correct_expression() {
        let cron = CronExpression::builder()
            .sundays_only()
            .hour(10)
            .minute(0);
        
        let expression = cron.build();
        // Builder uses ISO 8601 (Sunday=7) but converts to cron format (Sunday=1) for compatibility
        assert_eq!(expression, "0 0 10 * * 1 *");
    }

    #[test]
    fn mondays_only_produces_correct_expression() {
        let cron = CronExpression::builder()
            .mondays_only()
            .hour(9)
            .minute(0);
        
        let expression = cron.build();
        // ISO 8601 Monday=1 converted to cron format Monday=2
        assert_eq!(expression, "0 0 9 * * 2 *");
    }

    #[test]
    fn tuesdays_only_produces_correct_expression() {
        let cron = CronExpression::builder()
            .tuesdays_only()
            .hour(14)
            .minute(0);
        
        let expression = cron.build();
        // ISO 8601 Tuesday=2 converted to cron format Tuesday=3
        assert_eq!(expression, "0 0 14 * * 3 *");
    }

    #[test]
    fn wednesdays_only_produces_correct_expression() {
        let cron = CronExpression::builder()
            .wednesdays_only()
            .hour(11)
            .minute(0);
        
        let expression = cron.build();
        // ISO 8601 Wednesday=3 converted to cron format Wednesday=4
        assert_eq!(expression, "0 0 11 * * 4 *");
    }

    #[test]
    fn thursdays_only_produces_correct_expression() {
        let cron = CronExpression::builder()
            .thursdays_only()
            .hour(15)
            .minute(0);
        
        let expression = cron.build();
        // ISO 8601 Thursday=4 converted to cron format Thursday=5
        assert_eq!(expression, "0 0 15 * * 5 *");
    }

    #[test]
    fn fridays_only_produces_correct_expression() {
        let cron = CronExpression::builder()
            .fridays_only()
            .hour(17)
            .minute(0);
        
        let expression = cron.build();
        // ISO 8601 Friday=5 converted to cron format Friday=6
        assert_eq!(expression, "0 0 17 * * 6 *");
    }

    #[test]
    fn saturdays_only_produces_correct_expression() {
        let cron = CronExpression::builder()
            .saturdays_only()
            .hour(8)
            .minute(0);
        
        let expression = cron.build();
        // ISO 8601 Saturday=6 converted to cron format Saturday=7
        assert_eq!(expression, "0 0 8 * * 7 *");
    }

    #[test]
    fn weekdays_only_still_works() {
        let cron = CronExpression::builder()
            .weekdays_only()
            .hours_range(9, 17)
            .minute(0);
        
        let expression = cron.build();
        // ISO 8601 Monday-Friday (1-5) converted to cron format (2-6)
        assert_eq!(expression, "0 0 9-17 * * 2-6 *");
    }

    #[test]
    fn weekends_only_still_works() {
        let cron = CronExpression::builder()
            .weekends_only()
            .hour(10)
            .minute(0);
        
        let expression = cron.build();
        // ISO 8601 Saturday=6, Sunday=7 converted to cron format Saturday=7, Sunday=1
        assert_eq!(expression, "0 0 10 * * 7,1 *");
    }

    #[test]
    fn sunday_schedule_executes_on_sunday() {
        let cron = CronExpression::builder()
            .sundays_only()
            .hour(10)
            .minute(0);
        
        // Start from a Saturday (Jan 6, 2024 is Saturday since Jan 1 is Monday)
        let saturday = Utc.with_ymd_and_hms(2024, 1, 6, 12, 0, 0).unwrap();
        let next = cron.next_after(&saturday, UTC).unwrap();
        
        assert_eq!(next.weekday(), chrono::Weekday::Sun);
        assert_eq!(next.hour(), 10);
        assert_eq!(next.minute(), 0);
        assert_eq!(next.day(), 7); // Next day is Sunday Jan 7
    }

    #[test]
    fn monday_schedule_executes_on_monday() {
        let cron = CronExpression::builder()
            .mondays_only()
            .hour(9)
            .minute(0);
        
        // Start from a Sunday (Jan 7, 2024)
        let sunday = Utc.with_ymd_and_hms(2024, 1, 7, 12, 0, 0).unwrap();
        let next = cron.next_after(&sunday, UTC).unwrap();
        
        assert_eq!(next.weekday(), chrono::Weekday::Mon);
        assert_eq!(next.hour(), 9);
        assert_eq!(next.minute(), 0);
        assert_eq!(next.day(), 8); // Next day is Monday Jan 8
    }

    #[test]
    fn friday_schedule_executes_on_friday() {
        let cron = CronExpression::builder()
            .fridays_only()
            .hour(17)
            .minute(0);
        
        // Start from a Thursday (Jan 4, 2024)
        let thursday = Utc.with_ymd_and_hms(2024, 1, 4, 18, 0, 0).unwrap();
        let next = cron.next_after(&thursday, UTC).unwrap();
        
        assert_eq!(next.weekday(), chrono::Weekday::Fri);
        assert_eq!(next.hour(), 17);
        assert_eq!(next.minute(), 0);
        assert_eq!(next.day(), 5); // Next day is Friday Jan 5
    }

    #[test]
    fn saturday_schedule_executes_on_saturday() {
        let cron = CronExpression::builder()
            .saturdays_only()
            .hour(8)
            .minute(0);
        
        // Start from a Friday (Jan 5, 2024)
        let friday = Utc.with_ymd_and_hms(2024, 1, 5, 12, 0, 0).unwrap();
        let next = cron.next_after(&friday, UTC).unwrap();
        
        assert_eq!(next.weekday(), chrono::Weekday::Sat);
        assert_eq!(next.hour(), 8);
        assert_eq!(next.minute(), 0);
        assert_eq!(next.day(), 6); // Next day is Saturday Jan 6
    }
}

mod quarterly_method {
    use super::*;

    #[test]
    fn quarterly_produces_correct_expression() {
        let cron = CronExpression::builder()
            .quarterly();
        
        let expression = cron.build();
        assert_eq!(expression, "0 0 0 1 1,4,7,10 * *");
    }

    #[test]
    fn quarterly_with_custom_hour() {
        let cron = CronExpression::builder()
            .quarterly()
            .hour(9);
        
        let expression = cron.build();
        assert_eq!(expression, "0 0 9 1 1,4,7,10 * *");
    }

    #[test]
    fn quarterly_with_custom_time() {
        let cron = CronExpression::builder()
            .quarterly()
            .hour(14)
            .minute(30);
        
        let expression = cron.build();
        assert_eq!(expression, "0 30 14 1 1,4,7,10 * *");
    }

    #[test]
    fn quarterly_executes_in_january() {
        let cron = CronExpression::builder()
            .quarterly();
        
        // Start from mid-December
        let december = Utc.with_ymd_and_hms(2024, 12, 15, 0, 0, 0).unwrap();
        let next = cron.next_after(&december, UTC).unwrap();
        
        assert_eq!(next.month(), 1);
        assert_eq!(next.day(), 1);
        assert_eq!(next.year(), 2025);
    }

    #[test]
    fn quarterly_executes_in_april() {
        let cron = CronExpression::builder()
            .quarterly();
        
        // Start from mid-February
        let february = Utc.with_ymd_and_hms(2024, 2, 15, 0, 0, 0).unwrap();
        let next = cron.next_after(&february, UTC).unwrap();
        
        assert_eq!(next.month(), 4);
        assert_eq!(next.day(), 1);
        assert_eq!(next.year(), 2024);
    }

    #[test]
    fn quarterly_executes_in_july() {
        let cron = CronExpression::builder()
            .quarterly();
        
        // Start from mid-May
        let may = Utc.with_ymd_and_hms(2024, 5, 15, 0, 0, 0).unwrap();
        let next = cron.next_after(&may, UTC).unwrap();
        
        assert_eq!(next.month(), 7);
        assert_eq!(next.day(), 1);
        assert_eq!(next.year(), 2024);
    }

    #[test]
    fn quarterly_executes_in_october() {
        let cron = CronExpression::builder()
            .quarterly();
        
        // Start from mid-August
        let august = Utc.with_ymd_and_hms(2024, 8, 15, 0, 0, 0).unwrap();
        let next = cron.next_after(&august, UTC).unwrap();
        
        assert_eq!(next.month(), 10);
        assert_eq!(next.day(), 1);
        assert_eq!(next.year(), 2024);
    }

    #[test]
    fn quarterly_advances_to_next_year() {
        let cron = CronExpression::builder()
            .quarterly();
        
        // Start from mid-November (after October)
        let november = Utc.with_ymd_and_hms(2024, 11, 15, 0, 0, 0).unwrap();
        let next = cron.next_after(&november, UTC).unwrap();
        
        assert_eq!(next.month(), 1);
        assert_eq!(next.day(), 1);
        assert_eq!(next.year(), 2025);
    }

    #[test]
    fn quarterly_multiple_executions() {
        let cron = CronExpression::builder()
            .quarterly();
        
        let start = Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap();
        let q1 = cron.next_after(&start, UTC).unwrap();
        let q2 = cron.next_after(&q1, UTC).unwrap();
        let q3 = cron.next_after(&q2, UTC).unwrap();
        let q4 = cron.next_after(&q3, UTC).unwrap();
        
        assert_eq!(q1.month(), 4);
        assert_eq!(q2.month(), 7);
        assert_eq!(q3.month(), 10);
        assert_eq!(q4.month(), 1);
        assert_eq!(q4.year(), 2025);
    }
}

mod method_combinations {
    use super::*;

    #[test]
    fn sundays_with_timezone() {
        use chrono_tz::US::Eastern;
        
        let cron = CronExpression::builder()
            .sundays_only()
            .hour(10)
            .minute(0)
            .with_timezone(Eastern);
        
        let expression = cron.build();
        // ISO 8601 Sunday=7 converted to cron format Sunday=1
        assert_eq!(expression, "0 0 10 * * 1 *");
    }

    #[test]
    fn fridays_with_jitter() {
        use std::time::Duration;
        
        let cron = CronExpression::builder()
            .fridays_only()
            .hour(17)
            .minute(0)
            .with_jitter(Duration::from_secs(300));
        
        let expression = cron.build();
        // ISO 8601 Friday=5 converted to cron format Friday=6
        assert_eq!(expression, "0 0 17 * * 6 *");
    }

    #[test]
    fn quarterly_with_blackout_dates() {
        use chrono::NaiveDate;
        
        let cron = CronExpression::builder()
            .quarterly()
            .exclude_date(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap());
        
        let expression = cron.build();
        assert_eq!(expression, "0 0 0 1 1,4,7,10 * *");
    }

    #[test]
    fn mondays_business_hours() {
        let cron = CronExpression::builder()
            .mondays_only()
            .hours_range(9, 17)
            .minutes_interval(30);
        
        let expression = cron.build();
        // ISO 8601 Monday=1 converted to cron format Monday=2
        assert_eq!(expression, "0 */30 9-17 * * 2 *");
    }

    #[test]
    fn wednesdays_midday() {
        let cron = CronExpression::builder()
            .wednesdays_only()
            .hour(12)
            .minute(0);
        
        let expression = cron.build();
        // ISO 8601 Wednesday=3 converted to cron format Wednesday=4
        assert_eq!(expression, "0 0 12 * * 4 *");
    }

    #[test]
    fn saturdays_morning() {
        let cron = CronExpression::builder()
            .saturdays_only()
            .hours_range(8, 12)
            .minute(0);
        
        let expression = cron.build();
        // ISO 8601 Saturday=6 converted to cron format Saturday=7
        assert_eq!(expression, "0 0 8-12 * * 7 *");
    }
}

mod validation_and_edge_cases {
    use super::*;

    #[test]
    fn all_weekday_methods_parse_successfully() {
        let expressions = vec![
            CronExpression::builder().sundays_only().build(),
            CronExpression::builder().mondays_only().build(),
            CronExpression::builder().tuesdays_only().build(),
            CronExpression::builder().wednesdays_only().build(),
            CronExpression::builder().thursdays_only().build(),
            CronExpression::builder().fridays_only().build(),
            CronExpression::builder().saturdays_only().build(),
        ];
        
        for expr in expressions {
            let result = foxtive_cron::contracts::ValidatedSchedule::parse(&expr);
            assert!(result.is_ok(), "Failed to parse: {}", expr);
        }
    }

    #[test]
    fn quarterly_parses_successfully() {
        let cron = CronExpression::builder().quarterly();
        let expression = cron.build();
        
        let result = foxtive_cron::contracts::ValidatedSchedule::parse(&expression);
        assert!(result.is_ok(), "Failed to parse quarterly: {}", expression);
    }

    #[test]
    fn quarterly_contains_all_four_months() {
        let cron = CronExpression::builder().quarterly();
        let expression = cron.build();
        
        assert!(expression.contains("1,4,7,10"));
    }

    #[test]
    fn weekday_methods_override_each_other() {
        // Last call should win
        let cron = CronExpression::builder()
            .mondays_only()
            .fridays_only()
            .minute(0);
        
        let expression = cron.build();
        // Fridays at every hour, ISO 8601 Friday=5 converted to cron format Friday=6
        assert_eq!(expression, "0 0 * * * 6 *");
    }

    #[test]
    fn sunday_is_one_in_cron_format() {
        let cron = CronExpression::builder().sundays_only();
        let expression = cron.build();
        
        // ISO 8601 Sunday=7 is converted to cron format Sunday=1
        assert!(expression.contains(" 1 "));
    }

    #[test]
    fn quarterly_can_be_overridden() {
        // Later month call should override quarterly
        let cron = CronExpression::builder()
            .quarterly()
            .month(Month::March);
        
        let expression = cron.build();
        assert!(expression.contains(" 3 "));
        assert!(!expression.contains("1,4,7,10"));
    }
}
