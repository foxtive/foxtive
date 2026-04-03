use foxtive_cron::contracts::{JobContract, ValidatedSchedule};
use foxtive_cron::{Cron, CronResult};
use std::borrow::Cow;
use std::time::Duration;
use async_trait::async_trait;

/// A job that has its own per-job concurrency limit.
/// This prevents too many instances of the same job from running
/// at the same time, even if the global scheduler limit is high.
struct HighLatencyJob {
    id: String,
    schedule: ValidatedSchedule,
}

#[async_trait]
impl JobContract for HighLatencyJob {
    async fn run(&self) -> CronResult<()> {
        println!("[{}] Starting high latency task...", self.id);
        // Simulate a long-running process (5 seconds)
        // With a 1-second schedule, this job would pile up
        // without a concurrency limit.
        tokio::time::sleep(Duration::from_secs(5)).await;
        println!("[{}] Long-running task completed.", self.id);
        Ok(())
    }

    fn id(&self) -> Cow<'_, str> { Cow::Borrowed(&self.id) }
    fn name(&self) -> Cow<'_, str> { Cow::Borrowed("High Latency Task") }
    fn schedule(&self) -> &ValidatedSchedule { &self.schedule }

    // This method limits the number of concurrent runs for this specific job to 2.
    // If 2 instances are running and a new one is scheduled, it will wait
    // for a slot to open up.
    fn concurrency_limit(&self) -> Option<usize> {
        Some(2)
    }
}

#[tokio::main]
async fn main() {
    // Start with a global concurrency limit of 10.
    // This allows up to 10 jobs of any type to run at once.
    let mut cron = Cron::builder()
        .with_global_concurrency_limit(10)
        .build();

    // Schedule a job every 1 second, but with a per-job limit of 2.
    // Since each job takes 5 seconds, we'll see 2 instances running
    // almost constantly, while the schedule attempts to fire every second.
    let latency_job = HighLatencyJob {
        id: "long-task".to_string(),
        schedule: ValidatedSchedule::parse("* * * * * * *").unwrap(),
    };
    cron.add_job(latency_job).unwrap();

    // Add a fast job to show it's not blocked by the slow job's limit.
    cron.add_job_fn("fast-job", "Fast Job", "*/2 * * * * * *", || async {
        println!("  Fast job: Tick-tock at {}", chrono::Utc::now());
        Ok(())
    }).unwrap();

    println!("Scheduler starting with concurrency limits...");
    cron.run().await;
}
