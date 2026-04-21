use foxtive_cron::builder::{CronExpression, Month, Weekday};
use chrono::{Datelike, TimeZone, Timelike, Utc};
use chrono_tz::UTC;

#[test]
fn test_basic_presets() {
    assert_eq!(CronExpression::builder().daily().build(), "0 0 0 * * * *");
    assert_eq!(CronExpression::builder().hourly().build(), "0 0 * * * * *");
    assert_eq!(CronExpression::builder().weekly().build(), "0 0 0 * * 1 *");
    assert_eq!(CronExpression::builder().monthly().build(), "0 0 0 1 * * *");
}

#[test]
fn test_custom_time_daily() {
    // 9 AM Daily
    let cron = CronExpression::builder().daily().hour(9).build();
    assert_eq!(cron, "0 0 9 * * * *");

    // 10:30 PM Daily
    let cron = CronExpression::builder().daily().hour(22).minute(30).build();
    assert_eq!(cron, "0 30 22 * * * *");
}

#[test]
fn test_complex_intervals() {
    // Every 15 minutes
    let cron = CronExpression::builder().minutes_interval(15).build();
    assert_eq!(cron, "0 */15 * * * * *");

    // Every 10 seconds
    let cron = CronExpression::builder().seconds_interval(10).build();
    assert_eq!(cron, "*/10 * * * * * *");
}

#[test]
fn test_ranges() {
    // Working hours (9-17) on weekdays
    let cron = CronExpression::builder()
        .hours_range(9, 17)
        .weekdays_only()
        .build();
    // Cron crate format: Monday=2 through Friday=6
    assert_eq!(cron, "0 * 9-17 * * 2-6 *");
}

#[test]
fn test_lists() {
    // Specific hours
    let cron = CronExpression::builder().hours_list(&[9, 12, 18, 21]).build();
    assert_eq!(cron, "0 * 9,12,18,21 * * * *");

    // Weekends (explicit list check)
    // Cron crate format: Saturday=7, Sunday=1
    let cron = CronExpression::builder().weekends_only().build();
    assert_eq!(cron, "0 * * * * 7,1 *");
}

#[test]
fn test_enums() {
    let cron = CronExpression::builder()
        .month(Month::December)
        .day_of_week(Weekday::Friday)
        .build();
    // Cron crate format: Friday=6
    assert_eq!(cron, "0 * * * 12 6 *");
}

#[test]
fn test_validation_errors() {
    // Invalid hour
    let res = CronExpression::builder().hour(24).to_validated();
    assert!(res.is_err());

    // Invalid minute
    let res = CronExpression::builder().minute(60).to_validated();
    assert!(res.is_err());

    // Invalid range order
    let res = CronExpression::builder().hours_range(20, 10).to_validated();
    assert!(res.is_err());

    // Invalid interval
    let res = CronExpression::builder().seconds_interval(0).to_validated();
    assert!(res.is_err());
}

#[test]
fn test_schedule_trait_logic() {
    use foxtive_cron::contracts::Schedule;

    // Run every day at 9 AM
    let cron = CronExpression::builder().daily().hour(9);

    // Start at 8 AM
    let start = Utc.with_ymd_and_hms(2024, 1, 1, 8, 0, 0).unwrap();
    let next = cron.next_after(&start, UTC).unwrap();

    assert_eq!(next.hour(), 9);
    assert_eq!(next.minute(), 0);
    assert_eq!(next.day(), 1);

    // If it's already 10 AM, next run should be tomorrow at 9 AM
    let after_start = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
    let next = cron.next_after(&after_start, UTC).unwrap();

    assert_eq!(next.hour(), 9);
    assert_eq!(next.day(), 2);
}

#[test]
fn test_leap_year_february() {
    // Run on Feb 29th
    let cron = CronExpression::builder()
        .month(Month::February)
        .day_of_month(29)
        .build();

    assert_eq!(cron, "0 * * 29 2 * *");

    let res = CronExpression::builder()
        .month(Month::February)
        .day_of_month(29)
        .to_validated();

    // cron-utils/cron crate usually validates this based on whether the day is possible in ANY year
    assert!(res.is_ok());
}
