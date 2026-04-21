use async_trait::async_trait;
use chrono::Utc;
use chrono_tz::Tz;
use foxtive_cron::contracts::{JobContract, Schedule, ValidatedSchedule};
use foxtive_cron::{Cron, CronResult};
use std::borrow::Cow;

/// A job that runs in a specific timezone.
/// This is useful for tasks that need to run at a specific local time,
/// regardless of the server's timezone or DST changes.
struct LocalizedJob {
    id: String,
    schedule: ValidatedSchedule,
    timezone: Tz,
}

#[async_trait]
impl JobContract for LocalizedJob {
    async fn run(&self) -> CronResult<()> {
        let now_utc = Utc::now();
        let now_local = now_utc.with_timezone(&self.timezone);
        println!(
            "[{}] Running task. UTC: {}, Local ({}): {}",
            self.id, now_utc, self.timezone, now_local
        );
        Ok(())
    }

    fn id(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.id)
    }
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("Localized Job")
    }
    fn schedule(&self) -> &dyn Schedule {
        &self.schedule
    }

    // Override the timezone method to specify the desired zone
    fn timezone(&self) -> Tz {
        self.timezone
    }
}

#[tokio::main]
async fn main() {
    let mut cron = Cron::new();

    // Schedule a job to run every 10 seconds in New York time
    let ny_tz: Tz = "America/New_York".parse().unwrap();
    let job_ny = LocalizedJob {
        id: "ny-task".to_string(),
        schedule: ValidatedSchedule::parse("*/10 * * * * * *").unwrap(), // Every 10 seconds for demo
        timezone: ny_tz,
    };
    cron.add_job(job_ny).unwrap();

    // Schedule another job in Tokyo time
    let tokyo_tz: Tz = "Asia/Tokyo".parse().unwrap();
    let job_tokyo = LocalizedJob {
        id: "tokyo-task".to_string(),
        schedule: ValidatedSchedule::parse("*/10 * * * * * *").unwrap(), // Every 10 seconds for demo
        timezone: tokyo_tz,
    };
    cron.add_job(job_tokyo).unwrap();

    println!("Scheduler starting with multiple time zones...");
    cron.run().await;
}
