use foxtive_cron::Cron;

/// Demonstrates real-world production scenarios with cron scheduling
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Foxtive Cron Real-World Examples ===\n");

    // Initialize tracing for better observability
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let mut cron = Cron::builder().with_global_concurrency_limit(10).build();

    // Scenario 1: Database backup every night at 2 AM
    println!("1. Setting up daily database backup at 2 AM...");
    cron.add_job_fn("db-backup", "Database Backup", "0 30 2 * * * *", || async {
        println!("   [BACKUP] Running database backup...");
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        println!("   [BACKUP] Backup completed successfully");
        Ok(())
    })?;

    // Scenario 2: Health check every 30 seconds
    println!("2. Setting up health check every 30 seconds...");
    cron.add_job_fn(
        "health-check",
        "Health Check",
        "*/30 * * * * * *",
        || async {
            println!("   [HEALTH] Service is healthy");
            Ok(())
        },
    )?;

    // Scenario 3: Cache cleanup every 6 hours
    println!("3. Setting up cache cleanup every 6 hours...");
    cron.add_job_fn(
        "cache-cleanup",
        "Cache Cleanup",
        "0 0 0,6,12,18 * * * *",
        || async {
            println!("   [CACHE] Cleaning expired cache entries...");
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            println!("   [CACHE] Cache cleanup done");
            Ok(())
        },
    )?;

    // Scenario 4: Business hours monitoring (weekdays 9-17, every 15 min)
    println!("4. Setting up business hours monitoring...");
    cron.add_job_fn(
        "biz-monitor",
        "Business Hours Monitor",
        "0 */15 9-17 * * 1-5 *",
        || async {
            println!("   [MONITOR] Checking business metrics...");
            Ok(())
        },
    )?;

    // Scenario 5: Monthly report generation on 1st at 9 AM
    println!("5. Setting up monthly report generation...");
    cron.add_job_fn(
        "monthly-report",
        "Monthly Report",
        "0 0 9 1 * * *",
        || async {
            println!("   [REPORT] Generating monthly analytics report...");
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            println!("   [REPORT] Report generated and sent to stakeholders");
            Ok(())
        },
    )?;

    // Scenario 6: API rate limit reset every hour
    println!("6. Setting up API rate limit reset...");
    cron.add_job_fn(
        "rate-limit-reset",
        "Rate Limit Reset",
        "0 0 * * * * *",
        || async {
            println!("   [RATE-LIMIT] Resetting API rate limits");
            Ok(())
        },
    )?;

    // Scenario 7: Log rotation on Sundays at midnight
    println!("7. Setting up weekly log rotation...");
    cron.add_job_fn("log-rotation", "Log Rotation", "0 0 0 * * 7 *", || async {
        println!("   [LOGS] Rotating application logs...");
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        println!("   [LOGS] Logs rotated and archived");
        Ok(())
    })?;

    // Scenario 8: ETL pipeline daily at 2:30 AM
    println!("8. Setting up ETL pipeline...");
    cron.add_job_fn("etl-pipeline", "ETL Pipeline", "0 30 2 * * * *", || async {
        println!("   [ETL] Extracting data from sources...");
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        println!("   [ETL] Transforming and loading data...");
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        println!("   [ETL] Pipeline completed successfully");
        Ok(())
    })?;

    // Scenario 9: Security scan nightly at 1 AM
    println!("9. Setting up nightly security scan...");
    cron.add_job_fn(
        "security-scan",
        "Security Scan",
        "0 0 1 * * * *",
        || async {
            println!("   [SECURITY] Running vulnerability scan...");
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
            println!("   [SECURITY] Scan complete - no vulnerabilities found");
            Ok(())
        },
    )?;

    // Scenario 10: Webhook retry queue every minute
    println!("10. Setting up webhook retry processor...");
    cron.add_job_fn(
        "webhook-retry",
        "Webhook Retry Queue",
        "0 * * * * * *",
        || async {
            println!("   [WEBHOOK] Processing failed webhook deliveries...");
            Ok(())
        },
    )?;

    println!("\n✅ All jobs scheduled successfully!");
    println!("📊 Total jobs: {}", cron.list_job_ids().len());
    println!("\n⏰ Scheduler running... Press Ctrl+C to stop\n");

    // Run scheduler in background
    let scheduler_handle = tokio::spawn(async move {
        cron.run().await;
    });

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;

    println!("\n🛑 Shutting down gracefully...");
    scheduler_handle.abort();

    Ok(())
}
