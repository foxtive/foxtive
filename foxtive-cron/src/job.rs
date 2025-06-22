use crate::contracts::JobContract;
use crate::CronResult;
use chrono::{DateTime, Utc};
use cron::Schedule;
use std::str::FromStr;
use std::sync::Arc;

#[derive(Clone)]
pub struct JobItem {
    schedule: Schedule,
    job: Arc<dyn JobContract>,
}

impl JobItem {
    pub fn new(job: Arc<dyn JobContract>) -> CronResult<Self> {
        let schedule = Schedule::from_str(&job.schedule())?;
        Ok(JobItem {
            schedule,
            job,
        })
    }

    pub fn name(&self) -> String {
        self.job.name()
    }

    pub fn next_run_time(&self) -> Option<DateTime<Utc>> {
        self.schedule.upcoming(Utc).next()
    }

    pub async fn run(&self) -> CronResult<()> {
        self.job.run().await
    }
}