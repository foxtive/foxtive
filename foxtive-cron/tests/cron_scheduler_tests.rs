mod common;
use common::*;
use foxtive_cron::Cron;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::timeout;

mod cron_scheduler {
    use super::*;

    #[test]
    fn new_creates_empty_scheduler() {
        let cron = Cron::new();
        let _ = cron;
        let _ = Cron::default();
    }

    #[test]
    fn add_job_fn_accepts_valid_schedule() {
        let mut cron = Cron::new();
        let result = cron.add_job_fn("id", "Name", "*/1 * * * * * *", || async { Ok(()) });
        assert!(result.is_ok());
    }

    #[test]
    fn add_job_fn_rejects_invalid_schedule() {
        let mut cron = Cron::new();
        let result = cron.add_job_fn("id", "Name", "bad schedule", || async { Ok(()) });
        assert!(result.is_err());
    }

    #[test]
    fn add_blocking_job_fn_accepts_valid_schedule() {
        let mut cron = Cron::new();
        let result = cron.add_blocking_job_fn("id", "Name", "*/1 * * * * * *", || Ok(()));
        assert!(result.is_ok());
    }

    #[test]
    fn add_blocking_job_fn_rejects_invalid_schedule() {
        let mut cron = Cron::new();
        let result = cron.add_blocking_job_fn("id", "Name", "nope", || Ok(()));
        assert!(result.is_err());
    }

    #[test]
    fn add_job_accepts_arc_job_contract() {
        let mut cron = Cron::new();
        let job = MockJob::new("mock", "*/1 * * * * * *");
        assert!(cron.add_job(job).is_ok());
    }

    #[test]
    fn multiple_jobs_can_be_registered() {
        let mut cron = Cron::new();
        for i in 0..5 {
            let job = MockJob::new(format!("mock-{}", i), "*/1 * * * * * *");
            cron.add_job(job)
                .unwrap_or_else(|_| panic!("failed on job {i}"));
        }
    }

    #[test]
    fn list_job_ids_returns_correct_ids() {
        let mut cron = Cron::new();
        cron.add_job_fn("job-1", "Job 1", "*/1 * * * * * *", || async { Ok(()) })
            .unwrap();
        cron.add_job_fn("job-2", "Job 2", "*/1 * * * * * *", || async { Ok(()) })
            .unwrap();

        let ids = cron.list_job_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"job-1".to_string()));
        assert!(ids.contains(&"job-2".to_string()));
    }

    #[tokio::test]
    async fn remove_job_removes_from_registry() {
        let mut cron = Cron::new();
        cron.add_job_fn("job-1", "Job 1", "*/1 * * * * * *", || async { Ok(()) })
            .unwrap();
        assert!(cron.remove_job("job-1").is_some());
        assert_eq!(cron.list_job_ids().len(), 0);
    }

    #[tokio::test]
    async fn trigger_job_executes_immediately() {
        let run_count = Arc::new(AtomicUsize::new(0));
        let run_count_clone = run_count.clone();

        let mut cron = Cron::new();
        cron.add_job_fn("job-1", "Job 1", "0 0 0 1 1 * *", move || {
            let count = run_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .unwrap();

        cron.trigger_job("job-1").await.unwrap();

        let result = timeout(Duration::from_secs(1), async {
            loop {
                if run_count.load(Ordering::SeqCst) >= 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;

        assert!(result.is_ok(), "manual trigger did not execute job");
    }

    #[tokio::test]
    async fn scheduler_executes_registered_job() {
        let run_count = Arc::new(AtomicUsize::new(0));
        let run_count_clone = run_count.clone();

        let mut cron = Cron::new();
        cron.add_job_fn("id", "Name", "*/1 * * * * * *", move || {
            let count = run_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .unwrap();

        let handle = tokio::spawn(async move {
            cron.run().await;
        });

        let result = timeout(Duration::from_secs(3), async {
            loop {
                if run_count.load(Ordering::SeqCst) >= 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;

        handle.abort();
        assert!(result.is_ok(), "job was not executed within 3 seconds");
    }

    #[tokio::test]
    async fn empty_scheduler_exits_promptly() {
        let mut cron = Cron::new();
        let result = timeout(Duration::from_millis(200), async move {
            cron.run().await;
        })
        .await;
        assert!(result.is_ok(), "empty scheduler did not exit in time");
    }

    #[tokio::test]
    async fn removed_job_stops_running() {
        let run_count = Arc::new(AtomicUsize::new(0));
        let run_count_clone = run_count.clone();

        let mut cron = Cron::new();
        cron.add_job_fn("job-1", "Job 1", "*/1 * * * * * *", move || {
            let count = run_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .unwrap();

        let run_count_for_loop = run_count.clone();
        let handle = tokio::spawn(async move {
            while run_count_for_loop.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            let (tx, mut rx) = tokio::sync::mpsc::channel(1);
            let mut cron_inner = cron;

            let handle_inner = tokio::spawn(async move {
                tokio::select! {
                    _ = cron_inner.run() => {},
                    _ = rx.recv() => {
                        cron_inner.remove_job("job-1");
                        cron_inner.run().await;
                    },
                }
            });

            tx.send(()).await.unwrap();
            tokio::time::sleep(Duration::from_secs(2)).await;
            handle_inner.abort();
        });

        tokio::time::sleep(Duration::from_secs(1)).await;
        let count_after_removal = run_count.load(Ordering::SeqCst);

        tokio::time::sleep(Duration::from_secs(2)).await;
        let final_count = run_count.load(Ordering::SeqCst);

        handle.abort();

        assert!(
            final_count <= count_after_removal + 1,
            "job continued to run after removal. count_after_removal: {}, final_count: {}",
            count_after_removal,
            final_count
        );
    }
}
