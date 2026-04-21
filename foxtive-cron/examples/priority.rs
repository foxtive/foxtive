use async_trait::async_trait;
use foxtive_cron::contracts::{JobContract, ValidatedSchedule, Schedule};
use foxtive_cron::{Cron, CronResult};
use std::borrow::Cow;
use std::time::Duration;

/// A simple job that prints its priority.
struct JobWithPriority {
    id: String,
    priority: i32,
    schedule: ValidatedSchedule,
}

#[async_trait]
impl JobContract for JobWithPriority {
    async fn run(&self) -> CronResult<()> {
        println!(
            "[{}] Executing (priority: {}). Timestamp: {}",
            self.id,
            self.priority,
            chrono::Utc::now()
        );
        // Small sleep so we don't spam too fast in the logs
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(())
    }

    fn id(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.id)
    }
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("Priority Job")
    }
    fn schedule(&self) -> &dyn Schedule {
        &self.schedule
    }

    // Set the job's priority. Higher values run first if scheduled at the same time.
    fn priority(&self) -> i32 {
        self.priority
    }
}

#[tokio::main]
async fn main() {
    // Limit concurrency to 1 so we can clearly see the execution order
    let mut cron = Cron::builder().with_global_concurrency_limit(1).build();

    // Schedule three jobs at exactly the same time (every 5 seconds)
    // with different priorities. The one with priority 100 should
    // fire first, followed by priority 50, then 0.
    let every_5_sec = ValidatedSchedule::parse("*/5 * * * * * *").unwrap();

    let high_prio = JobWithPriority {
        id: "high-prio-task".to_string(),
        priority: 100,
        schedule: every_5_sec.clone(),
    };
    let med_prio = JobWithPriority {
        id: "med-prio-task".to_string(),
        priority: 50,
        schedule: every_5_sec.clone(),
    };
    let low_prio = JobWithPriority {
        id: "low-prio-task".to_string(),
        priority: 0,
        schedule: every_5_sec.clone(),
    };

    // Add them in random order to show priority still works
    cron.add_job(med_prio).unwrap();
    cron.add_job(low_prio).unwrap();
    cron.add_job(high_prio).unwrap();

    println!("Scheduler starting with prioritized jobs...");
    cron.run().await;
}
