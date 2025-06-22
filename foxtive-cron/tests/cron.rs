use std::sync::{Arc, Mutex};
use std::time::Duration;

use foxtive_cron::contracts::JobContract;
use foxtive_cron::Cron;
use foxtive_cron::CronResult;
use tokio::time::sleep;

#[tokio::test]
async fn test_add_and_run_async_job() {
    let triggered = Arc::new(Mutex::new(false));
    let triggered_clone = triggered.clone();

    let mut cron = Cron::new();
    cron.add_job_fn("test_async", "*/1 * * * * * *", move || {
        let triggered = triggered_clone.clone();
        async move {
            *triggered.lock().unwrap() = true;
            Ok(())
        }
    })
    .unwrap();

    tokio::spawn(async move {
        cron.run().await;
    });

    sleep(Duration::from_secs(2)).await;
    assert!(
        *triggered.lock().unwrap(),
        "Async job should have been triggered"
    );
}

#[tokio::test]
async fn test_add_and_run_blocking_job() {
    let triggered = Arc::new(Mutex::new(false));
    let triggered_clone = triggered.clone();

    let mut cron = Cron::new();
    cron.add_blocking_job_fn("blocking", "*/1 * * * * * *", move || {
        *triggered_clone.lock().unwrap() = true;
        Ok(())
    })
    .unwrap();

    tokio::spawn(async move {
        cron.run().await;
    });

    sleep(Duration::from_secs(2)).await;
    assert!(
        *triggered.lock().unwrap(),
        "Blocking job should have been triggered"
    );
}

#[tokio::test]
async fn test_job_reschedules_itself() {
    let counter = Arc::new(Mutex::new(0));
    let counter_clone = counter.clone();

    let mut cron = Cron::new();
    cron.add_job_fn("repeat", "*/1 * * * * * *", move || {
        let counter = counter_clone.clone();
        async move {
            let mut count = counter.lock().unwrap();
            *count += 1;
            Ok(())
        }
    })
    .unwrap();

    tokio::spawn(async move {
        cron.run().await;
    });

    sleep(Duration::from_secs(3)).await;
    let count = *counter.lock().unwrap();
    assert!(
        count >= 2,
        "Job should have run at least twice, but ran {} times",
        count
    );
}

#[tokio::test]
async fn test_failed_job_does_not_stop_scheduler() {
    let success_flag = Arc::new(Mutex::new(false));
    let success_flag_clone = success_flag.clone();

    let mut cron = Cron::new();

    // Add a job that always fails
    cron.add_job_fn("fail", "*/1 * * * * * *", || async {
        Err(anyhow::anyhow!("Intentional failure"))
    })
    .unwrap();

    // Add a job that succeeds
    cron.add_job_fn("success", "*/1 * * * * * *", move || {
        let success_flag = success_flag_clone.clone();
        async move {
            *success_flag.lock().unwrap() = true;
            Ok(())
        }
    })
    .unwrap();

    tokio::spawn(async move {
        cron.run().await;
    });

    sleep(Duration::from_secs(2)).await;
    assert!(
        *success_flag.lock().unwrap(),
        "Scheduler should continue after a failed job"
    );
}

#[tokio::test]
async fn test_add_custom_job_trait() {
    struct DummyJob {
        counter: Arc<Mutex<u32>>,
    }

    #[async_trait::async_trait]
    impl JobContract for DummyJob {
        fn name(&self) -> String {
            "dummy".to_string()
        }

        fn schedule(&self) -> String {
            "*/1 * * * * * *".to_string()
        }

        async fn run(&self) -> CronResult<()> {
            let mut count = self.counter.lock().unwrap();
            *count += 1;
            Ok(())
        }
    }

    let counter = Arc::new(Mutex::new(0));
    let job = Arc::new(DummyJob {
        counter: counter.clone(),
    });

    let mut cron = Cron::new();
    cron.add_job(job).unwrap();

    tokio::spawn(async move {
        cron.run().await;
    });

    sleep(Duration::from_secs(2)).await;
    assert!(
        *counter.lock().unwrap() > 0,
        "Custom job trait should be called"
    );
}
