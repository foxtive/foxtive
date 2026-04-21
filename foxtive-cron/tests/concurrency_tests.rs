mod common;
use common::*;
use foxtive_cron::Cron;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

mod concurrency {
    use super::*;

    #[tokio::test]
    async fn global_concurrency_limit_is_enforced() {
        let run_count = Arc::new(AtomicUsize::new(0));
        let active_count = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));

        let mut cron = Cron::new().with_global_concurrency_limit(2);

        for i in 0..5 {
            let run_count = run_count.clone();
            let active_count = active_count.clone();
            let max_active = max_active.clone();

            cron.add_job_fn(format!("job-{}", i), "Job", "*/1 * * * * * *", move || {
                let run_count = run_count.clone();
                let active_count = active_count.clone();
                let max_active = max_active.clone();
                async move {
                    let current = active_count.fetch_add(1, Ordering::SeqCst) + 1;
                    loop {
                        let prev = max_active.load(Ordering::SeqCst);
                        if current <= prev
                            || max_active
                                .compare_exchange(prev, current, Ordering::SeqCst, Ordering::SeqCst)
                                .is_ok()
                        {
                            break;
                        }
                    }

                    tokio::time::sleep(Duration::from_millis(100)).await;
                    run_count.fetch_add(1, Ordering::SeqCst);
                    active_count.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .unwrap();
        }

        let handle = tokio::spawn(async move {
            cron.run().await;
        });

        tokio::time::sleep(Duration::from_secs(2)).await;
        handle.abort();

        assert!(
            max_active.load(Ordering::SeqCst) <= 2,
            "global concurrency limit exceeded: {}",
            max_active.load(Ordering::SeqCst)
        );
        assert!(run_count.load(Ordering::SeqCst) > 0);
    }

    #[tokio::test]
    async fn per_job_concurrency_limit_is_enforced() {
        let run_count = Arc::new(AtomicUsize::new(0));
        let active_count = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));

        let mut cron = Cron::new();
        let run_count_clone = run_count.clone();
        let active_count_clone = active_count.clone();
        let max_active_clone = max_active.clone();

        let job = Arc::new(MockJob::new("limited-job", "* * * * * * *").with_concurrency_limit(1));

        // Using a custom wrapper to track concurrency for per-job limit
        struct LimitedJobWrapper {
            inner: Arc<MockJob>,
            active_count: Arc<AtomicUsize>,
            max_active: Arc<AtomicUsize>,
            run_count: Arc<AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl foxtive_cron::contracts::JobContract for LimitedJobWrapper {
            async fn run(&self) -> foxtive_cron::CronResult<()> {
                let current = self.active_count.fetch_add(1, Ordering::SeqCst) + 1;
                loop {
                    let prev = self.max_active.load(Ordering::SeqCst);
                    if current <= prev
                        || self
                            .max_active
                            .compare_exchange(prev, current, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                    {
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
                self.run_count.fetch_add(1, Ordering::SeqCst);
                self.active_count.fetch_sub(1, Ordering::SeqCst);
                Ok(())
            }
            fn id(&self) -> std::borrow::Cow<'_, str> {
                self.inner.id()
            }
            fn name(&self) -> std::borrow::Cow<'_, str> {
                self.inner.name()
            }
            fn schedule(&self) -> &dyn foxtive_cron::contracts::Schedule {
                self.inner.schedule()
            }
            fn concurrency_limit(&self) -> Option<usize> {
                self.inner.concurrency_limit()
            }
        }

        cron.add_job(LimitedJobWrapper {
            inner: job,
            active_count: active_count_clone,
            max_active: max_active_clone,
            run_count: run_count_clone,
        })
        .unwrap();

        let handle = tokio::spawn(async move {
            cron.run().await;
        });

        tokio::time::sleep(Duration::from_secs(2)).await;
        handle.abort();

        assert_eq!(
            max_active.load(Ordering::SeqCst),
            1,
            "per-job concurrency limit exceeded"
        );
    }
}
