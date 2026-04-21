use foxtive_cron::builder::{CronExpression, Month, Weekday};

/// Demonstrates builder API edge cases and validation scenarios
fn main() {
    println!("=== Builder Edge Cases & Validation Examples ===\n");

    // Example 1: Boundary values
    println!("1. Boundary Value Testing:");
    let min_values = CronExpression::builder()
        .second(0)
        .minute(0)
        .hour(0)
        .day_of_month(1)
        .month(Month::January);

    let max_values = CronExpression::builder()
        .second(59)
        .minute(59)
        .hour(23)
        .day_of_month(31)
        .month(Month::December);

    println!("   Minimum values: {}", min_values.build());
    println!("   Maximum values: {}", max_values.build());
    println!();

    // Example 2: Single value vs intervals
    println!("2. Single Values vs Intervals:");
    let single_minute = CronExpression::builder().minute(30).hourly();

    let interval_minutes = CronExpression::builder().minutes_interval(30).hourly();

    println!("   Single (at :30):   {}", single_minute.build());
    println!("   Interval (/30):    {}", interval_minutes.build());
    println!();

    // Example 3: Overlapping ranges
    println!("3. Range Combinations:");
    let morning_range = CronExpression::builder().hours_range(6, 12).daily();

    let afternoon_range = CronExpression::builder().hours_range(12, 18).daily();

    let evening_range = CronExpression::builder().hours_range(18, 23).daily();

    println!("   Morning (6-12):    {}", morning_range.build());
    println!("   Afternoon (12-18): {}", afternoon_range.build());
    println!("   Evening (18-23):   {}", evening_range.build());
    println!();

    // Example 4: List vs range equivalence
    println!("4. List vs Range Equivalence:");
    let as_list = CronExpression::builder()
        .hours_list(&[1, 2, 3, 4, 5])
        .daily();

    let as_range = CronExpression::builder().hours_range(1, 5).daily();

    println!("   As list [1,2,3,4,5]: {}", as_list.build());
    println!("   As range 1-5:        {}", as_range.build());
    println!("   Both produce same schedule\n");

    // Example 5: Sparse scheduling
    println!("5. Sparse Scheduling Patterns:");
    let quarterly = CronExpression::builder()
        .month(Month::March)
        .day_of_month(1);

    let semi_annual = CronExpression::builder()
        .month(Month::June)
        .day_of_month(30)
        .hour(17);

    println!("   Quarterly:         {}", quarterly.build());
    println!("   Semi-annual:       {}", semi_annual.build());
    println!();

    // Example 6: Dense scheduling
    println!("6. High-Frequency Scheduling:");
    let every_10_sec = CronExpression::builder().seconds_interval(10);

    let every_5_min = CronExpression::builder().minutes_interval(5);

    let every_hour = CronExpression::builder().hourly();

    println!("   Every 10 seconds:  {}", every_10_sec.build());
    println!("   Every 5 minutes:   {}", every_5_min.build());
    println!("   Every hour:        {}", every_hour.build());
    println!();

    // Example 7: Weekday combinations
    println!("7. Weekday Pattern Variations:");
    let weekdays = CronExpression::builder().weekdays_only().hour(9);

    let weekends = CronExpression::builder()
        .day_of_week(Weekday::Saturday)
        .day_of_week(Weekday::Sunday)
        .hour(10);

    let mon_fri = CronExpression::builder()
        .day_of_week(Weekday::Monday)
        .day_of_week(Weekday::Friday)
        .hour(9);

    println!("   Weekdays only:     {}", weekdays.build());
    println!("   Weekends only:     {}", weekends.build());
    println!("   Mon & Fri:         {}", mon_fri.build());
    println!();

    // Example 8: Month groupings
    println!("8. Seasonal Month Groupings:");
    let spring = CronExpression::builder()
        .month(Month::March)
        .day_of_month(1);

    let summer = CronExpression::builder().month(Month::June).day_of_month(1);

    let fall = CronExpression::builder()
        .month(Month::September)
        .day_of_month(1);

    let winter = CronExpression::builder()
        .month(Month::December)
        .day_of_month(1);

    println!("   Spring start:      {}", spring.build());
    println!("   Summer start:      {}", summer.build());
    println!("   Fall start:        {}", fall.build());
    println!("   Winter start:      {}", winter.build());
    println!();

    // Example 9: Preset comparisons
    println!("9. Preset Methods Comparison:");
    let hourly_preset = CronExpression::builder().hourly();

    let daily_preset = CronExpression::builder().daily();

    let weekly_preset = CronExpression::builder().weekly();

    let monthly_preset = CronExpression::builder().monthly();

    println!("   hourly():          {}", hourly_preset.build());
    println!("   daily():           {}", daily_preset.build());
    println!("   weekly():          {}", weekly_preset.build());
    println!("   monthly():         {}", monthly_preset.build());
    println!();

    // Example 10: Chaining order effects
    println!("10. Method Chaining Order:");
    let chain_a = CronExpression::builder().daily().hour(9).minute(30);

    let chain_b = CronExpression::builder().minute(30).hour(9).daily();

    println!("   daily().hour(9).minute(30): {}", chain_a.build());
    println!("   minute(30).hour(9).daily(): {}", chain_b.build());
    println!("   Order doesn't affect result\n");

    // Example 11: Complex real-world validation
    println!("11. Real-world Validation Scenarios:");
    let banking_hours = CronExpression::builder()
        .weekdays_only()
        .hours_range(9, 15)
        .minutes_interval(30);

    let retail_hours = CronExpression::builder().daily().hours_range(10, 21);

    let restaurant_hours = CronExpression::builder()
        .daily()
        .hours_list(&[11, 12, 13, 14, 15, 17, 18, 19, 20, 21]);

    println!("   Banking (9-3):     {}", banking_hours.build());
    println!("   Retail (10-9):     {}", retail_hours.build());
    println!("   Restaurant hours:  {}", restaurant_hours.build());
    println!();

    // Example 12: Edge case - all wildcards
    println!("12. Wildcard Patterns:");
    let all_wildcards = CronExpression::builder().every_second();

    let minute_wildcard = CronExpression::builder().every_minute();

    println!("   Every second:      {}", all_wildcards.build());
    println!("   Every minute:      {}", minute_wildcard.build());
    println!();

    // Example 13: Specific day of month edge cases
    println!("13. Day of Month Edge Cases:");
    let first_day = CronExpression::builder().day_of_month(1).monthly();

    let mid_month = CronExpression::builder().day_of_month(15).monthly();

    let last_possible = CronExpression::builder().day_of_month(31).monthly();

    println!("   1st of month:      {}", first_day.build());
    println!("   15th of month:     {}", mid_month.build());
    println!("   31st of month:     {}", last_possible.build());
    println!("   Note: Skips months with <31 days\n");

    // Example 14: Timezone interaction patterns
    println!("14. Timezone Interaction Patterns:");
    use chrono_tz::US::Eastern;

    let tz_with_range = CronExpression::builder()
        .hours_range(9, 17)
        .with_timezone(Eastern);

    let tz_with_list = CronExpression::builder()
        .hours_list(&[9, 12, 15])
        .with_timezone(Eastern);

    let tz_with_interval = CronExpression::builder()
        .hours_list(&[9, 13, 17])
        .with_timezone(Eastern);

    println!("   TZ + range:        {}", tz_with_range.build());
    println!("   TZ + list:         {}", tz_with_list.build());
    println!("   TZ + intervals:    {}", tz_with_interval.build());
    println!();

    // Example 15: Jitter with different frequencies
    println!("15. Jitter Across Frequencies:");
    let low_freq_jitter = CronExpression::builder()
        .daily()
        .hour(2)
        .with_jitter(std::time::Duration::from_secs(3600)); // ±1 hour

    let high_freq_jitter = CronExpression::builder()
        .seconds_interval(30)
        .with_jitter(std::time::Duration::from_secs(5)); // ±5 seconds

    println!("   Daily ±1hr:        {}", low_freq_jitter.build());
    println!("   30s ±5sec:         {}", high_freq_jitter.build());
    println!("   Jitter scales with frequency\n");

    println!("=== All edge case examples completed! ===");
}
