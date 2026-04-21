use foxtive_cron::builder::{CronExpression, Weekday};
use chrono::NaiveDate;
use std::time::Duration;

/// Demonstrates blackout dates and jitter for production resilience
fn main() {
    println!("=== Blackout Dates & Jitter Examples ===\n");

    // Example 1: Holiday-excluding business schedule
    println!("1. Business hours excluding US holidays:");
    let us_holidays = vec![
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),   // New Year
        NaiveDate::from_ymd_opt(2024, 7, 4).unwrap(),   // Independence Day
        NaiveDate::from_ymd_opt(2024, 12, 25).unwrap(), // Christmas
        NaiveDate::from_ymd_opt(2024, 11, 28).unwrap(), // Thanksgiving
    ];
    
    let mut business_schedule = CronExpression::builder()
        .weekdays_only()
        .hours_range(9, 17)
        .minutes_interval(30);
    
    for date in &us_holidays {
        business_schedule = business_schedule.exclude_date(*date);
    }
    
    println!("   Expression: {}", business_schedule.build());
    println!("   Excludes {} holidays", us_holidays.len());
    println!("   Runs every 30 min during business hours on non-holidays\n");

    // Example 2: Maintenance window exclusions
    println!("2. Service monitoring with maintenance windows:");
    let maintenance_dates = [
        NaiveDate::from_ymd_opt(2024, 3, 15).unwrap(),
        NaiveDate::from_ymd_opt(2024, 6, 15).unwrap(),
        NaiveDate::from_ymd_opt(2024, 9, 15).unwrap(),
        NaiveDate::from_ymd_opt(2024, 12, 15).unwrap(),
    ];
    
    let monitoring = CronExpression::builder()
        .every_minute()
        .exclude_date(maintenance_dates[0])
        .exclude_date(maintenance_dates[1])
        .exclude_date(maintenance_dates[2])
        .exclude_date(maintenance_dates[3])
        .with_jitter(Duration::from_secs(10));
    
    println!("   Expression: {}", monitoring.build());
    println!("   Skips quarterly maintenance dates");
    println!("   ±10s jitter prevents thundering herd\n");

    // Example 3: E-commerce flash sale exclusions
    println!("3. Regular promotions excluding flash sales:");
    let flash_sale_dates = [
        NaiveDate::from_ymd_opt(2024, 11, 29).unwrap(), // Black Friday
        NaiveDate::from_ymd_opt(2024, 12, 2).unwrap(),  // Cyber Monday
        NaiveDate::from_ymd_opt(2024, 12, 26).unwrap(), // Boxing Day
    ];
    
    let regular_promo = CronExpression::builder()
        .daily()
        .hours_list(&[10, 14, 18])
        .exclude_date(flash_sale_dates[0])
        .exclude_date(flash_sale_dates[1])
        .exclude_date(flash_sale_dates[2]);
    
    println!("   Expression: {}", regular_promo.build());
    println!("   Runs at 10 AM, 2 PM, 6 PM");
    println!("   Disabled during major sales events\n");

    // Example 4: Jitter for distributed systems
    println!("4. Distributed cache warming with jitter:");
    let cache_warm = CronExpression::builder()
        .hours_list(&[0, 4, 8, 12, 16, 20])
        .minute(0)
        .with_jitter(Duration::from_secs(300));  // ±5 minutes
    
    println!("   Expression: {}", cache_warm.build());
    println!("   6 times daily with ±5 min random offset");
    println!("   Prevents all nodes from warming simultaneously\n");

    // Example 5: Compliance audit with blackout periods
    println!("5. Compliance audits excluding audit freeze periods:");
    let freeze_periods = vec![
        // Q1 freeze
        NaiveDate::from_ymd_opt(2024, 3, 25).unwrap(),
        NaiveDate::from_ymd_opt(2024, 3, 26).unwrap(),
        NaiveDate::from_ymd_opt(2024, 3, 27).unwrap(),
        NaiveDate::from_ymd_opt(2024, 3, 28).unwrap(),
        NaiveDate::from_ymd_opt(2024, 3, 29).unwrap(),
        // Q2 freeze
        NaiveDate::from_ymd_opt(2024, 6, 24).unwrap(),
        NaiveDate::from_ymd_opt(2024, 6, 25).unwrap(),
        NaiveDate::from_ymd_opt(2024, 6, 26).unwrap(),
        NaiveDate::from_ymd_opt(2024, 6, 27).unwrap(),
        NaiveDate::from_ymd_opt(2024, 6, 28).unwrap(),
    ];
    
    let mut compliance_audit = CronExpression::builder()
        .day_of_week(Weekday::Friday)
        .hour(16);
    
    for date in &freeze_periods {
        compliance_audit = compliance_audit.exclude_date(*date);
    }
    
    let compliance_audit = compliance_audit.with_jitter(Duration::from_secs(600));
    
    println!("   Expression: {}", compliance_audit.build());
    println!("   Weekly Friday 4 PM audits");
    println!("   Excludes {} days of audit freeze", freeze_periods.len());
    println!("   ±10 min jitter for security\n");

    // Example 6: Batch processing with holiday awareness
    println!("6. Financial batch processing:");
    let market_holidays = vec![
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),  // MLK Day
        NaiveDate::from_ymd_opt(2024, 2, 19).unwrap(),  // Presidents Day
        NaiveDate::from_ymd_opt(2024, 5, 27).unwrap(),  // Memorial Day
        NaiveDate::from_ymd_opt(2024, 7, 4).unwrap(),
        NaiveDate::from_ymd_opt(2024, 9, 2).unwrap(),   // Labor Day
        NaiveDate::from_ymd_opt(2024, 11, 28).unwrap(),
        NaiveDate::from_ymd_opt(2024, 11, 29).unwrap(),
        NaiveDate::from_ymd_opt(2024, 12, 25).unwrap(),
    ];
    
    let mut batch_process = CronExpression::builder()
        .weekdays_only()
        .hours_list(&[8, 12, 16, 20]);
    
    for date in &market_holidays {
        batch_process = batch_process.exclude_date(*date);
    }
    
    let batch_process = batch_process.with_jitter(Duration::from_secs(180));
    
    println!("   Expression: {}", batch_process.build());
    println!("   4x daily on trading days");
    println!("   Excludes {} market holidays", market_holidays.len());
    println!("   ±3 min jitter for load distribution\n");

    // Example 7: Backup strategy with exclusion logic
    println!("7. Database backup with smart exclusions:");
    let no_backup_dates = vec![
        NaiveDate::from_ymd_opt(2024, 12, 24).unwrap(),  // Christmas Eve
        NaiveDate::from_ymd_opt(2024, 12, 25).unwrap(),  // Christmas
        NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),  // New Year's Eve
        NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),    // New Year's Day
    ];
    
    let mut backup_schedule = CronExpression::builder()
        .daily()
        .hour(2)
        .minute(30);
    
    for date in &no_backup_dates {
        backup_schedule = backup_schedule.exclude_date(*date);
    }
    
    let backup_schedule = backup_schedule.with_jitter(Duration::from_secs(600));
    
    println!("   Expression: {}", backup_schedule.build());
    println!("   Daily 2:30 AM backups");
    println!("   Skips {} holiday period dates", no_backup_dates.len());
    println!("   ±10 min jitter to avoid storage contention\n");

    // Example 8: API rate limit reset with jitter
    println!("8. API rate limiting with randomized resets:");
    let rate_limit_reset = CronExpression::builder()
        .hourly()
        .with_jitter(Duration::from_secs(30));  // ±30 seconds
    
    println!("   Expression: {}", rate_limit_reset.build());
    println!("   Hourly resets with ±30s variation");
    println!("   Prevents synchronized client retries\n");

    // Example 9: Log rotation with blackout dates
    println!("9. Log rotation excluding high-traffic periods:");
    let high_traffic_dates = vec![
        NaiveDate::from_ymd_opt(2024, 11, 29).unwrap(),  // Black Friday
        NaiveDate::from_ymd_opt(2024, 12, 2).unwrap(),   // Cyber Monday
        NaiveDate::from_ymd_opt(2024, 12, 23).unwrap(),  // Pre-Christmas
        NaiveDate::from_ymd_opt(2024, 12, 24).unwrap(),  // Christmas Eve
    ];
    
    let mut log_rotation = CronExpression::builder()
        .daily()
        .hour(3);
    
    for date in &high_traffic_dates {
        log_rotation = log_rotation.exclude_date(*date);
    }
    
    let log_rotation = log_rotation.with_jitter(Duration::from_secs(900));
    
    println!("   Expression: {}", log_rotation.build());
    println!("   Daily 3 AM rotation");
    println!("   Skips {} high-traffic dates", high_traffic_dates.len());
    println!("   ±15 min jitter for I/O distribution\n");

    // Example 10: Health checks with progressive jitter
    println!("10. Multi-tier health check strategy:");
    let critical_health = CronExpression::builder()
        .seconds_interval(30)
        .with_jitter(Duration::from_secs(2));  // Minimal jitter
    
    let standard_health = CronExpression::builder()
        .minutes_interval(5)
        .with_jitter(Duration::from_secs(30));  // Moderate jitter
    
    let summary_health = CronExpression::builder()
        .hourly()
        .with_jitter(Duration::from_secs(300));  // Significant jitter
    
    println!("   Critical (30s):     {} - ±2s jitter", critical_health.build());
    println!("   Standard (5min):    {} - ±30s jitter", standard_health.build());
    println!("   Summary (hourly):   {} - ±5min jitter", summary_health.build());
    println!("   Progressive jitter based on criticality\n");

    println!("=== All blackout & jitter examples completed! ===");
}
