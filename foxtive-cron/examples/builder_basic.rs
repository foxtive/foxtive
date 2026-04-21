use chrono_tz::US::Eastern;
use foxtive_cron::builder::{CronExpression, Month, Weekday};
use std::time::Duration;

/// Demonstrates building cron expressions using the fluent builder API
fn main() {
    println!("=== Foxtive Cron Expression Builder Examples ===\n");

    // Example 1: Simple daily schedule
    let daily_backup = CronExpression::builder().daily().hour(2).minute(30);

    println!("1. Daily backup at 2:30 AM:");
    println!("   Expression: {}\n", daily_backup.build());

    // Example 2: Business hours monitoring
    let business_monitoring = CronExpression::builder()
        .weekdays_only()
        .hours_range(9, 17)
        .minutes_interval(15);

    println!("2. Business hours monitoring (every 15 min):");
    println!("   Expression: {}\n", business_monitoring.build());

    // Example 3: Monthly report with timezone
    let monthly_report = CronExpression::builder()
        .monthly()
        .hour(9)
        .with_timezone(Eastern);

    println!("3. Monthly report at 9 AM Eastern:");
    println!("   Expression: {}\n", monthly_report.build());

    // Example 4: Health check with jitter
    let health_check = CronExpression::builder()
        .seconds_interval(30)
        .with_jitter(Duration::from_secs(5));

    println!("4. Health check every 30s with 5s jitter:");
    println!("   Expression: {}\n", health_check.build());

    // Example 5: Complex schedule with blackout dates
    use chrono::NaiveDate;

    let complex_schedule = CronExpression::builder()
        .weekdays_only()
        .hours_range(8, 18)
        .minutes_interval(30)
        .exclude_date(NaiveDate::from_ymd_opt(2024, 12, 25).unwrap())
        .exclude_date(NaiveDate::from_ymd_opt(2024, 12, 26).unwrap())
        .with_jitter(Duration::from_secs(60));

    println!("5. Complex weekday schedule with holidays:");
    println!("   Expression: {}", complex_schedule.build());
    println!("   Blackout dates: 2024-12-25, 2024-12-26\n");

    // Example 6: Specific days of week
    let weekly_meeting = CronExpression::builder()
        .day_of_week(Weekday::Monday)
        .hour(10)
        .minute(0);

    println!("6. Weekly meeting Monday at 10 AM:");
    println!("   Expression: {}\n", weekly_meeting.build());

    // Example 7: Multiple specific hours
    let digest_emails = CronExpression::builder()
        .daily()
        .hours_list(&[8, 12, 18])
        .minute(0);

    println!("7. Digest emails at 8 AM, 12 PM, 6 PM:");
    println!("   Expression: {}\n", digest_emails.build());

    // Example 8: Quarterly schedule
    let quarterly_review = CronExpression::builder()
        .day_of_month(1)
        .month(Month::January)
        .hour(9);

    println!("8. Quarterly review (Jan 1st at 9 AM):");
    println!("   Expression: {}\n", quarterly_review.build());

    // Example 9: Every minute during specific range
    let stock_market = CronExpression::builder()
        .every_minute()
        .hours_range(9, 16)
        .weekdays_only();

    println!("9. Stock market data (every min, 9-16 ET weekdays):");
    println!("   Expression: {}\n", stock_market.build());

    // Example 10: Off-peak batch processing
    let batch_processing = CronExpression::builder()
        .minutes_interval(30)
        .hours_list(&[22, 23, 0, 1, 2, 3, 4, 5, 6]);

    println!("10. Off-peak batch processing (10 PM - 6 AM):");
    println!("    Expression: {}\n", batch_processing.build());

    println!("=== All examples completed successfully! ===");
}
