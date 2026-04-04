use anyhow::Result;
use async_trait::async_trait;
use chrono::{Timelike, Utc};
use foxtive_supervisor::Supervisor;
use foxtive_supervisor::SupervisedTask;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time::{sleep, Duration};
use tracing::{info, Level};
use tracing_subscriber;

/// Task that runs every 2 seconds
struct FrequentTask {
    execution_count: Arc<AtomicUsize>,
}

#[async_trait]
impl SupervisedTask for FrequentTask {
    fn id(&self) -> &'static str {
        "frequent-task"
    }

    fn cron_schedule(&self) -> Option<&'static str> {
        // Run every 2 seconds
        Some("*/2 * * * * * *")
    }

    async fn run(&self) -> Result<()> {
        let count = self.execution_count.fetch_add(1, Ordering::SeqCst) + 1;
        let now = Utc::now().format("%H:%M:%S");
        info!(count, time = %now, "Frequent task executed");
        Ok(())
    }
}

/// Task that runs every 5 seconds with rate limiting
struct RateLimitedTask {
    execution_count: Arc<AtomicUsize>,
}

#[async_trait]
impl SupervisedTask for RateLimitedTask {
    fn id(&self) -> &'static str {
        "rate-limited-task"
    }

    fn cron_schedule(&self) -> Option<&'static str> {
        // Try to run every second (but rate limited to 3 seconds)
        Some("*/1 * * * * * *")
    }

    fn min_restart_interval(&self) -> Option<Duration> {
        // Enforce minimum 3 seconds between executions
        Some(Duration::from_secs(3))
    }

    async fn run(&self) -> Result<()> {
        let count = self.execution_count.fetch_add(1, Ordering::SeqCst) + 1;
        let now = Utc::now().format("%H:%M:%S");
        info!(count, time = %now, "Rate-limited task executed");
        
        // Simulate occasional failures
        if count % 4 == 0 {
            anyhow::bail!("Simulated failure");
        }
        
        Ok(())
    }
}

/// Task with initial delay and jitter
struct DelayedTask {
    execution_count: Arc<AtomicUsize>,
}

#[async_trait]
impl SupervisedTask for DelayedTask {
    fn id(&self) -> &'static str {
        "delayed-task"
    }

    fn cron_schedule(&self) -> Option<&'static str> {
        // Run every 10 seconds
        Some("*/10 * * * * * *")
    }

    fn initial_delay(&self) -> Option<Duration> {
        // Wait 3 seconds before first execution
        Some(Duration::from_secs(3))
    }

    fn jitter(&self) -> Option<(Duration, Duration)> {
        // Add 0-2 seconds of random jitter
        Some((Duration::from_millis(0), Duration::from_secs(2)))
    }

    async fn run(&self) -> Result<()> {
        let count = self.execution_count.fetch_add(1, Ordering::SeqCst) + 1;
        let now = Utc::now().format("%H:%M:%S");
        info!(count, time = %now, "Delayed task with jitter executed");
        Ok(())
    }
}

/// Task with time window restrictions
struct BusinessHoursTask {
    execution_count: Arc<AtomicUsize>,
}

#[async_trait]
impl SupervisedTask for BusinessHoursTask {
    fn id(&self) -> &'static str {
        "business-hours-task"
    }

    fn cron_schedule(&self) -> Option<&'static str> {
        // Try to run every minute
        Some("0 * * * * * *")
    }

    fn execution_time_window(&self) -> Option<(Option<u8>, Option<u8>)> {
        // Only execute between 9 AM and 5 PM UTC (for demo, using current hour)
        // In real scenarios, this would restrict actual execution times
        None // Disabled for demo - would be Some((Some(9), Some(17))) for 9-17 UTC
    }

    async fn run(&self) -> Result<()> {
        let count = self.execution_count.fetch_add(1, Ordering::SeqCst) + 1;
        let now = Utc::now().format("%H:%M:%S");
        let hour = Utc::now().hour();
        info!(count, time = %now, hour, "Business hours task executed");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting cron scheduling example");
    info!("This demonstrates various scheduling features:");
    info!("  - Basic cron expressions");
    info!("  - Rate limiting");
    info!("  - Initial delays with jitter");
    info!("  - Time window restrictions");

    let frequent_count = Arc::new(AtomicUsize::new(0));
    let rate_limited_count = Arc::new(AtomicUsize::new(0));
    let delayed_count = Arc::new(AtomicUsize::new(0));
    let business_count = Arc::new(AtomicUsize::new(0));

    let supervisor = Supervisor::new()
        .add(FrequentTask {
            execution_count: frequent_count.clone(),
        })
        .add(RateLimitedTask {
            execution_count: rate_limited_count.clone(),
        })
        .add(DelayedTask {
            execution_count: delayed_count.clone(),
        })
        .add(BusinessHoursTask {
            execution_count: business_count.clone(),
        });

    info!("\nStarting supervisor with cron-scheduled tasks...");
    info!("Tasks will run according to their schedules");
    info!("Press Ctrl+C to stop\n");

    // Run until interrupted or for a demo period
    tokio::select! {
        result = supervisor.start_and_wait_any() => {
            if let Err(e) = result {
                info!(error = %e, "Supervisor encountered an error");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("\nReceived shutdown signal");
        }
        _ = sleep(Duration::from_secs(30)) => {
            info!("\nDemo period completed (30 seconds)");
        }
    }

    // Print summary
    info!("\n=== Execution Summary ===");
    info!(
        frequent = frequent_count.load(Ordering::SeqCst),
        "Frequent task (every 2s) executions"
    );
    info!(
        rate_limited = rate_limited_count.load(Ordering::SeqCst),
        "Rate-limited task (min 3s interval) executions"
    );
    info!(
        delayed = delayed_count.load(Ordering::SeqCst),
        "Delayed task (3s initial + jitter) executions"
    );
    info!(
        business = business_count.load(Ordering::SeqCst),
        "Business hours task executions"
    );

    info!("\nExample completed!");

    Ok(())
}
