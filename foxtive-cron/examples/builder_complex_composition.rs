use foxtive_cron::builder::{CronExpression, Month, Weekday};

/// Demonstrates complex field composition and advanced patterns
fn main() {
    println!("=== Complex Field Composition Examples ===\n");

    // Example 1: Multiple ranges in one expression
    println!("1. Split-shift monitoring (morning + evening):");
    let split_shift = CronExpression::builder()
        .weekdays_only()
        .hours_list(&[6, 7, 8, 9, 10, 16, 17, 18, 19, 20])
        .minutes_interval(15);

    println!("   Expression: {}", split_shift.build());
    println!("   Morning: 6-10 AM, Evening: 4-8 PM\n");

    // Example 2: Specific days across multiple months
    println!("2. Quarterly reviews on specific dates:");
    let quarterly_review = CronExpression::builder()
        .day_of_month(15)
        .month(Month::March)
        .hour(14);

    println!("   Expression: {}", quarterly_review.build());
    println!("   15th of Mar, Jun, Sep, Dec at 2 PM\n");

    // Example 3: Complex weekly pattern
    println!("3. Team standups (Mon/Wed/Fri at different times):");
    let mon_standup = CronExpression::builder()
        .day_of_week(Weekday::Monday)
        .hour(9)
        .minute(30);

    let wed_standup = CronExpression::builder()
        .day_of_week(Weekday::Wednesday)
        .hour(10)
        .minute(0);

    let fri_standup = CronExpression::builder()
        .day_of_week(Weekday::Friday)
        .hour(11)
        .minute(30);

    println!("   Monday:    {} - 9:30 AM", mon_standup.build());
    println!("   Wednesday: {} - 10:00 AM", wed_standup.build());
    println!("   Friday:    {} - 11:30 AM", fri_standup.build());
    println!();

    // Example 4: Seasonal business hours
    println!("4. Retail seasonal hours:");
    let summer_hours = CronExpression::builder()
        .month(Month::June)
        .weekdays_only()
        .hours_range(8, 20)
        .minutes_interval(30);

    let winter_hours = CronExpression::builder()
        .month(Month::December)
        .weekdays_only()
        .hours_range(10, 18)
        .minutes_interval(30);

    println!("   Summer (Jun-Aug): {}", summer_hours.build());
    println!("   Winter (Dec-Feb): {}", winter_hours.build());
    println!();

    // Example 5: Multi-interval scheduling
    println!("5. Variable frequency monitoring:");
    let peak_monitoring = CronExpression::builder()
        .weekdays_only()
        .hours_range(9, 17)
        .minutes_interval(5);

    let offpeak_monitoring = CronExpression::builder()
        .weekdays_only()
        .hours_list(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 18, 19, 20, 21, 22, 23])
        .minutes_interval(30);

    println!(
        "   Peak hours (9-17):    {} - every 5 min",
        peak_monitoring.build()
    );
    println!(
        "   Off-peak hours:       {} - every 30 min",
        offpeak_monitoring.build()
    );
    println!();

    // Example 6: End-of-month processing
    println!("6. End-of-month financial close:");
    let eom_process = CronExpression::builder()
        .day_of_month(28)
        .hour(23)
        .minute(0);

    println!("   Expression: {}", eom_process.build());
    println!("   Runs on 28th at 11 PM (covers all months)\n");

    // Example 7: Specific weekday patterns
    println!("7. Bi-weekly payroll (every other Friday):");
    let payroll = CronExpression::builder()
        .day_of_week(Weekday::Friday)
        .hours_list(&[9, 14])
        .minute(0);

    println!("   Expression: {}", payroll.build());
    println!("   Every Friday at 9 AM and 2 PM\n");

    // Example 8: Academic semester schedule
    println!("8. University class schedule pattern:");
    let fall_semester = CronExpression::builder()
        .month(Month::September)
        .weekdays_only()
        .hours_range(8, 17)
        .minutes_interval(60);

    let spring_semester = CronExpression::builder()
        .month(Month::January)
        .weekdays_only()
        .hours_range(8, 17)
        .minutes_interval(60);

    println!("   Fall semester:   {}", fall_semester.build());
    println!("   Spring semester: {}", spring_semester.build());
    println!();

    // Example 9: Shift rotation pattern
    println!("9. Three-shift rotation coverage:");
    let morning_shift = CronExpression::builder()
        .daily()
        .hours_range(6, 14)
        .minutes_interval(60);

    let afternoon_shift = CronExpression::builder()
        .daily()
        .hours_range(14, 22)
        .minutes_interval(60);

    let night_shift = CronExpression::builder()
        .daily()
        .hours_list(&[22, 23, 0, 1, 2, 3, 4, 5])
        .minutes_interval(60);

    println!("   Morning (6-14):   {}", morning_shift.build());
    println!("   Afternoon (14-22):{}", afternoon_shift.build());
    println!("   Night (22-6):     {}", night_shift.build());
    println!();

    // Example 10: Fiscal year quarters
    println!("10. Fiscal quarter boundaries:");
    let q1_start = CronExpression::builder()
        .month(Month::January)
        .day_of_month(1)
        .hour(0);

    let q2_start = CronExpression::builder()
        .month(Month::April)
        .day_of_month(1)
        .hour(0);

    let q3_start = CronExpression::builder()
        .month(Month::July)
        .day_of_month(1)
        .hour(0);

    let q4_start = CronExpression::builder()
        .month(Month::October)
        .day_of_month(1)
        .hour(0);

    println!("   Q1 Start: {}", q1_start.build());
    println!("   Q2 Start: {}", q2_start.build());
    println!("   Q3 Start: {}", q3_start.build());
    println!("   Q4 Start: {}", q4_start.build());
    println!();

    // Example 11: Complex list combinations
    println!("11. Restaurant peak hour staffing:");
    let lunch_rush = CronExpression::builder()
        .weekdays_only()
        .hours_range(11, 14)
        .minutes_interval(15);

    let dinner_rush = CronExpression::builder()
        .weekdays_only()
        .hours_range(17, 21)
        .minutes_interval(15);

    let weekend_brunch = CronExpression::builder()
        .day_of_week(Weekday::Saturday)
        .hours_range(9, 14)
        .minutes_interval(30);

    println!("   Lunch rush:      {}", lunch_rush.build());
    println!("   Dinner rush:     {}", dinner_rush.build());
    println!("   Weekend brunch:  {}", weekend_brunch.build());
    println!();

    // Example 12: Data retention policies
    println!("12. Tiered data retention cleanup:");
    let hourly_cleanup = CronExpression::builder().hourly().minute(0);

    let daily_cleanup = CronExpression::builder().daily().hour(1);

    let weekly_cleanup = CronExpression::builder()
        .day_of_week(Weekday::Sunday)
        .hour(2);

    let monthly_cleanup = CronExpression::builder().monthly().day_of_month(1).hour(3);

    println!("   Hourly temp files:  {}", hourly_cleanup.build());
    println!("   Daily old logs:     {}", daily_cleanup.build());
    println!("   Weekly archives:    {}", weekly_cleanup.build());
    println!("   Monthly purges:     {}", monthly_cleanup.build());
    println!();

    println!("=== All complex composition examples completed! ===");
}
