use foxtive_cron::builder::{CronExpression, Month, Weekday};

/// Demonstrates builder patterns for DevOps and infrastructure automation
fn main() {
    println!("=== DevOps & Infrastructure Automation Examples ===\n");

    // Example 1: CI/CD pipeline triggers
    println!("1. CI/CD Pipeline Schedules:");
    let nightly_build = CronExpression::builder()
        .daily()
        .hour(2)
        .minute(0);
    
    let weekly_integration = CronExpression::builder()
        .day_of_week(Weekday::Sunday)
        .hour(3)
        .minute(0);
    
    let monthly_release = CronExpression::builder()
        .monthly()
        .day_of_month(1)
        .hour(4)
        .minute(0);
    
    println!("   Nightly builds:      {}", nightly_build.build());
    println!("   Weekly integration:  {}", weekly_integration.build());
    println!("   Monthly releases:    {}", monthly_release.build());
    println!();

    // Example 2: Infrastructure scaling
    println!("2. Auto-scaling Schedules:");
    let scale_up_morning = CronExpression::builder()
        .weekdays_only()
        .hour(7)
        .minute(30);
    
    let scale_down_evening = CronExpression::builder()
        .weekdays_only()
        .hour(19)
        .minute(0);
    
    let weekend_scale_down = CronExpression::builder()
        .day_of_week(Weekday::Saturday)
        .hour(0)
        .minute(0);
    
    println!("   Scale up (7:30 AM):  {}", scale_up_morning.build());
    println!("   Scale down (7 PM):   {}", scale_down_evening.build());
    println!("   Weekend minimal:     {}", weekend_scale_down.build());
    println!();

    // Example 3: Backup rotation strategy
    println!("3. Multi-tier Backup Strategy:");
    let incremental_backup = CronExpression::builder()
        .daily()
        .hour(1)
        .minute(0);
    
    let full_backup = CronExpression::builder()
        .day_of_week(Weekday::Sunday)
        .hour(2)
        .minute(0);
    
    let archive_backup = CronExpression::builder()
        .monthly()
        .day_of_month(1)
        .hour(3)
        .minute(0);
    
    println!("   Incremental (daily): {}", incremental_backup.build());
    println!("   Full (weekly):       {}", full_backup.build());
    println!("   Archive (monthly):   {}", archive_backup.build());
    println!();

    // Example 4: Certificate management
    println!("4. SSL Certificate Lifecycle:");
    let cert_check = CronExpression::builder()
        .daily()
        .hour(6)
        .minute(0);
    
    let cert_renewal = CronExpression::builder()
        .monthly()
        .day_of_month(15)
        .hour(10)
        .minute(0);
    
    println!("   Daily expiry check:  {}", cert_check.build());
    println!("   Monthly renewal:     {}", cert_renewal.build());
    println!();

    // Example 5: Log aggregation and analysis
    println!("5. Log Management Pipeline:");
    let log_rotation = CronExpression::builder()
        .daily()
        .hour(0)
        .minute(30);
    
    let log_aggregation = CronExpression::builder()
        .hourly()
        .minute(15);
    
    let log_analysis = CronExpression::builder()
        .daily()
        .hour(8)
        .minute(0);
    
    let log_archival = CronExpression::builder()
        .day_of_week(Weekday::Sunday)
        .hour(4)
        .minute(0);
    
    println!("   Rotation (daily):    {}", log_rotation.build());
    println!("   Aggregation (hourly):{}", log_aggregation.build());
    println!("   Analysis (daily):    {}", log_analysis.build());
    println!("   Archival (weekly):   {}", log_archival.build());
    println!();

    // Example 6: Database maintenance
    println!("6. Database Maintenance Schedule:");
    let stats_update = CronExpression::builder()
        .daily()
        .hour(3)
        .minute(0);
    
    let index_rebuild = CronExpression::builder()
        .day_of_week(Weekday::Sunday)
        .hour(4)
        .minute(0);
    
    let vacuum_analyze = CronExpression::builder()
        .daily()
        .hour(5)
        .minute(0);
    
    let backup_verify = CronExpression::builder()
        .day_of_week(Weekday::Saturday)
        .hour(6)
        .minute(0);
    
    println!("   Update stats:        {}", stats_update.build());
    println!("   Rebuild indexes:     {}", index_rebuild.build());
    println!("   Vacuum analyze:      {}", vacuum_analyze.build());
    println!("   Verify backups:      {}", backup_verify.build());
    println!();

    // Example 7: Monitoring and alerting
    println!("7. Monitoring Check Intervals:");
    let critical_checks = CronExpression::builder()
        .seconds_interval(30);
    
    let standard_checks = CronExpression::builder()
        .minutes_interval(5);
    
    let summary_reports = CronExpression::builder()
        .hourly()
        .minute(0);
    
    let daily_digest = CronExpression::builder()
        .daily()
        .hour(9)
        .minute(0);
    
    println!("   Critical (30s):      {}", critical_checks.build());
    println!("   Standard (5min):     {}", standard_checks.build());
    println!("   Hourly summary:      {}", summary_reports.build());
    println!("   Daily digest:        {}", daily_digest.build());
    println!();

    // Example 8: Container orchestration
    println!("8. Kubernetes Job Scheduling:");
    let job_cleanup = CronExpression::builder()
        .hourly()
        .minute(30);
    
    let pod_autoscale = CronExpression::builder()
        .minutes_interval(2);
    
    let node_drain = CronExpression::builder()
        .monthly()
        .day_of_month(1)
        .hour(2)
        .minute(0);
    
    println!("   Job cleanup:         {}", job_cleanup.build());
    println!("   Pod autoscaling:     {}", pod_autoscale.build());
    println!("   Node maintenance:    {}", node_drain.build());
    println!();

    // Example 9: Security scanning
    println!("9. Security Audit Schedule:");
    let vuln_scan = CronExpression::builder()
        .daily()
        .hour(1)
        .minute(0);
    
    let dependency_check = CronExpression::builder()
        .day_of_week(Weekday::Monday)
        .hour(10)
        .minute(0);
    
    let compliance_audit = CronExpression::builder()
        .monthly()
        .day_of_month(1)
        .hour(14)
        .minute(0);
    
    let penetration_test = CronExpression::builder()
        .month(Month::January)
        .day_of_month(1)
        .hour(9)
        .minute(0);
    
    println!("   Vulnerability scan:  {}", vuln_scan.build());
    println!("   Dependency check:    {}", dependency_check.build());
    println!("   Compliance audit:    {}", compliance_audit.build());
    println!("   Penetration test:    {}", penetration_test.build());
    println!();

    // Example 10: Cache management
    println!("10. Cache Invalidation Strategy:");
    let cache_warm = CronExpression::builder()
        .hours_list(&[0, 6, 12, 18])
        .minute(0);
    
    let cache_purge = CronExpression::builder()
        .daily()
        .hour(4)
        .minute(0);
    
    let cache_stats = CronExpression::builder()
        .hourly()
        .minute(45);
    
    println!("   Cache warming:       {}", cache_warm.build());
    println!("   Cache purge:         {}", cache_purge.build());
    println!("   Cache statistics:    {}", cache_stats.build());
    println!();

    // Example 11: Deployment windows
    println!("11. Safe Deployment Windows:");
    let weekday_deploy = CronExpression::builder()
        .weekdays_only()
        .hours_range(10, 16)
        .minute(0);
    
    let emergency_deploy = CronExpression::builder()
        .every_minute();
    
    println!("   Standard window:     {} - 10 AM-4 PM weekdays", weekday_deploy.build());
    println!("   Emergency:           {} - Anytime", emergency_deploy.build());
    println!();

    // Example 12: Cost optimization
    println!("12. Cloud Cost Optimization:");
    let dev_shutdown = CronExpression::builder()
        .weekdays_only()
        .hour(20)
        .minute(0);
    
    let dev_startup = CronExpression::builder()
        .weekdays_only()
        .hour(7)
        .minute(0);
    
    let weekend_shutdown = CronExpression::builder()
        .day_of_week(Weekday::Friday)
        .hour(20)
        .minute(0);
    
    let weekend_startup = CronExpression::builder()
        .day_of_week(Weekday::Monday)
        .hour(7)
        .minute(0);
    
    let cost_report = CronExpression::builder()
        .day_of_week(Weekday::Monday)
        .hour(9)
        .minute(0);
    
    println!("   Dev shutdown:        {}", dev_shutdown.build());
    println!("   Dev startup:         {}", dev_startup.build());
    println!("   Weekend off:         {}", weekend_shutdown.build());
    println!("   Weekend on:          {}", weekend_startup.build());
    println!("   Cost report:         {}", cost_report.build());
    println!();

    println!("=== All DevOps examples completed! ===");
}
