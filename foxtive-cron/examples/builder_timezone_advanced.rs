use foxtive_cron::builder::{CronExpression, Weekday};
use chrono_tz::{US::Pacific, Europe::London, Asia::Tokyo};
use std::time::Duration;

/// Demonstrates advanced timezone-aware scheduling patterns
fn main() {
    println!("=== Advanced Timezone Scheduling Examples ===\n");

    // Example 1: Multi-region deployment sync
    println!("1. Multi-region deployment synchronization:");
    let us_west = CronExpression::builder()
        .daily()
        .hour(3)
        .with_timezone(Pacific);
    
    let eu_west = CronExpression::builder()
        .daily()
        .hour(11)
        .with_timezone(London);
    
    let asia_east = CronExpression::builder()
        .daily()
        .hour(20)
        .with_timezone(Tokyo);
    
    println!("   US West (Pacific):  {} - 3 AM PST", us_west.build());
    println!("   EU West (London):   {} - 11 AM GMT", eu_west.build());
    println!("   Asia East (Tokyo):  {} - 8 PM JST", asia_east.build());
    println!("   All run at ~11:00 UTC simultaneously\n");

    // Example 2: DST-safe scheduling
    println!("2. DST-safe business hours monitoring:");
    let dst_safe = CronExpression::builder()
        .weekdays_only()
        .hours_range(9, 17)
        .minutes_interval(30)
        .with_timezone(London);
    
    println!("   Expression: {}", dst_safe.build());
    println!("   Automatically adjusts for British Summer Time\n");

    // Example 3: Cross-timezone reporting
    println!("3. Global daily report at end of business day:");
    let global_report = CronExpression::builder()
        .weekdays_only()
        .hour(18)
        .with_timezone(Pacific);
    
    println!("   Pacific: {} - 6 PM PST/PDT", global_report.build());
    println!("   Runs at different UTC times depending on DST\n");

    // Example 4: Regional maintenance windows
    println!("4. Regional maintenance windows (non-overlapping):");
    let maintenance_us = CronExpression::builder()
        .day_of_week(Weekday::Sunday)
        .hour(2)
        .with_timezone(Pacific);
    
    let maintenance_eu = CronExpression::builder()
        .day_of_week(Weekday::Sunday)
        .hour(3)
        .with_timezone(London);
    
    let maintenance_asia = CronExpression::builder()
        .day_of_week(Weekday::Sunday)
        .hour(4)
        .with_timezone(Tokyo);
    
    println!("   US Maintenance:      {} - Sunday 2 AM PST", maintenance_us.build());
    println!("   EU Maintenance:      {} - Sunday 3 AM GMT", maintenance_eu.build());
    println!("   Asia Maintenance:    {} - Sunday 4 AM JST", maintenance_asia.build());
    println!("   Staggered to avoid global outage\n");

    // Example 5: Market hours across exchanges
    println!("5. Stock market hours monitoring:");
    let nyse = CronExpression::builder()
        .weekdays_only()
        .hours_range(9, 16)
        .minutes_interval(5)
        .with_timezone(Pacific);  // NYSE is Eastern, but showing Pacific conversion
    
    let lse = CronExpression::builder()
        .weekdays_only()
        .hours_range(8, 16)
        .minutes_interval(5)
        .with_timezone(London);
    
    let tse = CronExpression::builder()
        .weekdays_only()
        .hours_range(9, 15)
        .minutes_interval(5)
        .with_timezone(Tokyo);
    
    println!("   NYSE (via Pacific):  {} - 9 AM-4 PM ET", nyse.build());
    println!("   LSE (London):        {} - 8 AM-4 PM GMT", lse.build());
    println!("   TSE (Tokyo):         {} - 9 AM-3 PM JST", tse.build());
    println!();

    // Example 6: Holiday-aware international schedule
    println!("6. Holiday-aware international operations:");
    use chrono::NaiveDate;
    
    let intl_ops = CronExpression::builder()
        .weekdays_only()
        .hours_range(8, 20)
        .minutes_interval(60)
        .with_timezone(London)
        .exclude_date(NaiveDate::from_ymd_opt(2024, 12, 25).unwrap())  // Christmas
        .exclude_date(NaiveDate::from_ymd_opt(2024, 12, 26).unwrap())  // Boxing Day
        .exclude_date(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap())    // New Year
        .with_jitter(Duration::from_secs(120));
    
    println!("   Expression: {}", intl_ops.build());
    println!("   Blackout dates: Dec 25, Dec 26, Jan 1");
    println!("   ±2 minute jitter to distribute load\n");

    // Example 7: Timezone-specific cleanup tasks
    println!("7. Region-specific data cleanup:");
    let cleanup_regions = [
        ("US-East", CronExpression::builder().daily().hour(1).with_timezone(Pacific).build()),
        ("EU-West", CronExpression::builder().daily().hour(2).with_timezone(London).build()),
        ("Asia-Pac", CronExpression::builder().daily().hour(3).with_timezone(Tokyo).build()),
    ];
    
    for (region, expr) in &cleanup_regions {
        println!("   {}: {} - Local midnight cleanup", region, expr);
    }
    println!();

    println!("=== All timezone examples completed! ===");
}
