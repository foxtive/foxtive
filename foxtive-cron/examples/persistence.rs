use async_trait::async_trait;
use foxtive_cron::contracts::{InMemoryJobStore, JobContract, JobStore, ValidatedSchedule, Schedule};
use foxtive_cron::{Cron, CronResult};
use std::borrow::Cow;
use std::sync::Arc;

/// A simple job that prints its execution count.
/// Since we use an `InMemoryJobStore`, the job state (last run time, failure count, etc.)
/// will be tracked by the scheduler.
struct StateTrackingJob {
    id: String,
    schedule: ValidatedSchedule,
}

#[async_trait]
impl JobContract for StateTrackingJob {
    async fn run(&self) -> CronResult<()> {
        println!("[{}] Executing job...", self.id);
        Ok(())
    }

    fn id(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.id)
    }
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("State Tracking Job")
    }
    fn schedule(&self) -> &dyn Schedule {
        &self.schedule
    }
}

#[tokio::main]
async fn main() {
    // Create an in-memory job store.
    // In a real application, you might implement JobStore for a database (e.g., PostgreSQL or SQLite).
    let job_store = Arc::new(InMemoryJobStore::new());

    // Configure the scheduler with the job store.
    let mut cron = Cron::builder().with_job_store(job_store.clone()).build();

    let job_id = "persistent-task";
    let job = StateTrackingJob {
        id: job_id.to_string(),
        schedule: ValidatedSchedule::parse("*/5 * * * * * *").unwrap(),
    };

    cron.add_job(job).unwrap();

    // Start a background task to periodically inspect the job store's state.
    let store_clone = job_store.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            if let Ok(Some(state)) = store_clone.get_state(job_id).await {
                println!("\n--- JOB STORE INSPECTION ---");
                println!("Job ID: {}", job_id);
                println!("Last Run: {:?}", state.last_run);
                println!("Last Success: {:?}", state.last_success);
                println!("Consecutive Failures: {}", state.consecutive_failures);
                println!("----------------------------\n");
            }
        }
    });

    println!("Scheduler starting with persistence tracking...");
    cron.run().await;
}
