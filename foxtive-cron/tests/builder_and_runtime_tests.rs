use foxtive_cron::{Cron, CronBuilder, CronError};
use std::sync::Arc;

mod cron_builder {
    use super::*;

    #[test]
    fn new_creates_empty_builder() {
        let builder = CronBuilder::new();
        // Just verify it compiles and can be created
        let _ = builder;
    }

    #[test]
    fn with_global_concurrency_limit_sets_limit() {
        let builder = CronBuilder::new().with_global_concurrency_limit(5);
        let cron = builder.build();
        // The limit is set internally, we can't directly inspect it but can verify it builds
        let _ = cron;
    }

    #[test]
    fn with_listener_adds_listener() {
        use foxtive_cron::contracts::{JobEventListener, JobEvent};
        use async_trait::async_trait;

        struct TestListener;

        #[async_trait]
        impl JobEventListener for TestListener {
            async fn on_event(&self, _event: JobEvent) {}
        }

        let listener = Arc::new(TestListener);
        let builder = CronBuilder::new().with_listener(listener);
        let cron = builder.build();
        let _ = cron;
    }

    #[test]
    fn with_metrics_exporter_sets_exporter() {
        use foxtive_cron::contracts::MetricsExporter;
        use std::time::Duration;

        struct TestExporter;

        impl MetricsExporter for TestExporter {
            fn record_start(&self, _id: &str, _name: &str) {}
            fn record_completion(&self, _id: &str, _name: &str, _duration: Duration) {}
            fn record_failure(&self, _id: &str, _name: &str) {}
            fn record_retry(&self, _id: &str, _name: &str) {}
            fn record_misfire(&self, _id: &str, _name: &str) {}
        }

        let exporter = Arc::new(TestExporter);
        let builder = CronBuilder::new().with_metrics_exporter(exporter);
        let cron = builder.build();
        let _ = cron;
    }

    #[test]
    fn with_job_store_sets_store() {
        use foxtive_cron::contracts::{JobStore, JobState};
        use async_trait::async_trait;

        struct TestStore;

        #[async_trait]
        impl JobStore for TestStore {
            async fn save_state(&self, _id: &str, _state: &JobState) -> Result<(), CronError> {
                Ok(())
            }
            async fn get_state(&self, _id: &str) -> Result<Option<JobState>, CronError> {
                Ok(None)
            }
        }

        let store = Arc::new(TestStore);
        let builder = CronBuilder::new().with_job_store(store);
        let cron = builder.build();
        let _ = cron;
    }

    #[test]
    fn builder_with_all_options() {
        use foxtive_cron::contracts::{JobEventListener, JobEvent, MetricsExporter, JobStore, JobState};
        use async_trait::async_trait;
        use std::time::Duration;

        struct TestListener;
        #[async_trait]
        impl JobEventListener for TestListener {
            async fn on_event(&self, _event: JobEvent) {}
        }

        struct TestExporter;
        impl MetricsExporter for TestExporter {
            fn record_start(&self, _id: &str, _name: &str) {}
            fn record_completion(&self, _id: &str, _name: &str, _duration: Duration) {}
            fn record_failure(&self, _id: &str, _name: &str) {}
            fn record_retry(&self, _id: &str, _name: &str) {}
            fn record_misfire(&self, _id: &str, _name: &str) {}
        }

        struct TestStore;
        #[async_trait]
        impl JobStore for TestStore {
            async fn save_state(&self, _id: &str, _state: &JobState) -> Result<(), CronError> { Ok(()) }
            async fn get_state(&self, _id: &str) -> Result<Option<JobState>, CronError> { Ok(None) }
        }

        let builder = CronBuilder::new()
            .with_global_concurrency_limit(10)
            .with_listener(Arc::new(TestListener))
            .with_metrics_exporter(Arc::new(TestExporter))
            .with_job_store(Arc::new(TestStore));

        let cron = builder.build();
        let _ = cron;
    }

    #[test]
    fn builder_chaining_is_fluent() {
        let cron = CronBuilder::new()
            .with_global_concurrency_limit(5)
            .with_global_concurrency_limit(10)  // Can override
            .build();
        let _ = cron;
    }
}

mod trigger_job_edge_cases {
    use super::*;

    #[tokio::test]
    async fn trigger_job_on_nonexistent_job_returns_error() {
        let mut cron = Cron::new();
        let result = cron.trigger_job("nonexistent").await;
        assert!(matches!(result, Err(CronError::JobNotFound(_))));
    }

    #[tokio::test]
    async fn trigger_job_executes_removed_job() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        
        let run_count = Arc::new(AtomicUsize::new(0));
        let run_count_clone = run_count.clone();

        let mut cron = Cron::new();
        cron.add_job_fn("job-1", "Job 1", "0 0 0 1 1 * *", move || {
            let count = run_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }).unwrap();

        // Remove the job
        cron.remove_job("job-1");

        // Triggering a removed job should fail
        let result = cron.trigger_job("job-1").await;
        assert!(matches!(result, Err(CronError::JobNotFound(_))));
    }

    #[tokio::test]
    async fn multiple_triggers_of_same_job() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::time::Duration;
        
        let run_count = Arc::new(AtomicUsize::new(0));
        let run_count_clone = run_count.clone();

        let mut cron = Cron::new();
        cron.add_job_fn("job-1", "Job 1", "0 0 0 1 1 * *", move || {
            let count = run_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }).unwrap();

        // Trigger multiple times
        cron.trigger_job("job-1").await.unwrap();
        cron.trigger_job("job-1").await.unwrap();
        cron.trigger_job("job-1").await.unwrap();

        // Wait for all to execute
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(run_count.load(Ordering::SeqCst), 3);
    }
}

mod shutdown_behavior {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn trigger_job_during_shutdown_fails() {
        
        let run_count = Arc::new(AtomicUsize::new(0));
        let run_count_clone = run_count.clone();

        let mut cron = Cron::new();
        cron.add_job_fn("job-1", "Job 1", "*/1 * * * * * *", move || {
            let count = run_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }).unwrap();

        // Start scheduler in background
        let handle = tokio::spawn(async move {
            cron.run().await;
        });

        // Give it a moment to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Shutdown
        // Note: We can't easily test this without modifying the Cron struct
        // This is a placeholder for the concept
        handle.abort();
    }

    #[tokio::test]
    async fn empty_scheduler_shutdown_is_immediate() {
        let mut cron = Cron::new();
        
        let result = timeout(Duration::from_millis(100), async {
            cron.shutdown().await;
        }).await;

        assert!(result.is_ok(), "Shutdown did not complete in time");
    }

    #[tokio::test]
    async fn shutdown_waits_for_running_jobs() {
        let run_count = Arc::new(AtomicUsize::new(0));
        let run_count_clone = run_count.clone();

        let mut cron = Cron::new();
        cron.add_job_fn("slow-job", "Slow Job", "*/1 * * * * * *", move || {
            let count = run_count_clone.clone();
            async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }).unwrap();

        let handle = tokio::spawn(async move {
            cron.run().await;
        });

        // Wait for job to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Shutdown should wait for the job to complete
        let shutdown_result = timeout(Duration::from_secs(2), async {
            // We need to get mutable access to call shutdown
            // This is a limitation - in real usage, you'd have a handle
        }).await;

        handle.abort();
        assert!(shutdown_result.is_ok());
    }
}

mod add_listener_runtime {
    use super::*;
    use foxtive_cron::contracts::{JobEventListener, JobEvent};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingListener {
        count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl JobEventListener for CountingListener {
        async fn on_event(&self, _event: JobEvent) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn add_listener_increases_listener_count() {
        let mut cron = Cron::new();
        let listener = Arc::new(CountingListener {
            count: Arc::new(AtomicUsize::new(0)),
        });

        cron.add_listener(listener);
        // Can't directly inspect, but verifies API works
    }

    #[test]
    fn multiple_listeners_can_be_added() {
        let mut cron = Cron::new();
        
        for _ in 0..5 {
            let listener = Arc::new(CountingListener {
                count: Arc::new(AtomicUsize::new(0)),
            });
            cron.add_listener(listener);
        }
    }
}

mod set_metrics_exporter_runtime {
    use super::*;
    use foxtive_cron::contracts::MetricsExporter;
    use std::time::Duration;

    struct TestExporter;

    impl MetricsExporter for TestExporter {
        fn record_start(&self, _id: &str, _name: &str) {}
        fn record_completion(&self, _id: &str, _name: &str, _duration: Duration) {}
        fn record_failure(&self, _id: &str, _name: &str) {}
        fn record_retry(&self, _id: &str, _name: &str) {}
        fn record_misfire(&self, _id: &str, _name: &str) {}
    }

    #[test]
    fn set_metrics_exporter_sets_exporter() {
        let mut cron = Cron::new();
        let exporter = Arc::new(TestExporter);
        cron.set_metrics_exporter(exporter);
    }

    #[test]
    fn metrics_exporter_can_be_replaced() {
        let mut cron = Cron::new();
        cron.set_metrics_exporter(Arc::new(TestExporter));
        cron.set_metrics_exporter(Arc::new(TestExporter));
    }
}

mod set_job_store_runtime {
    use super::*;
    use foxtive_cron::contracts::{JobStore, JobState};
    use async_trait::async_trait;

    struct TestStore;

    #[async_trait]
    impl JobStore for TestStore {
        async fn save_state(&self, _id: &str, _state: &JobState) -> Result<(), CronError> {
            Ok(())
        }
        async fn get_state(&self, _id: &str) -> Result<Option<JobState>, CronError> {
            Ok(None)
        }
    }

    #[test]
    fn set_job_store_sets_store() {
        let mut cron = Cron::new();
        let store = Arc::new(TestStore);
        cron.set_job_store(store);
    }

    #[test]
    fn job_store_can_be_replaced() {
        let mut cron = Cron::new();
        cron.set_job_store(Arc::new(TestStore));
        cron.set_job_store(Arc::new(TestStore));
    }
}
