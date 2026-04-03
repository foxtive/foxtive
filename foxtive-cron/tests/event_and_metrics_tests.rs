use foxtive_cron::contracts::{JobEvent, JobEventListener, MetricsExporter};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use chrono::Utc;

mod job_event {
    use super::*;

    #[test]
    fn started_variant_contains_id_and_name() {
        let event = JobEvent::Started {
            id: "job-1".to_string(),
            name: "Test Job".to_string(),
        };
        
        match event {
            JobEvent::Started { id, name } => {
                assert_eq!(id, "job-1");
                assert_eq!(name, "Test Job");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn completed_variant_contains_duration() {
        let event = JobEvent::Completed {
            id: "job-1".to_string(),
            name: "Test Job".to_string(),
            duration: Duration::from_secs(5),
        };
        
        match event {
            JobEvent::Completed { duration, .. } => {
                assert_eq!(duration, Duration::from_secs(5));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn failed_variant_contains_error_message() {
        let event = JobEvent::Failed {
            id: "job-1".to_string(),
            name: "Test Job".to_string(),
            error: "Something went wrong".to_string(),
        };
        
        match event {
            JobEvent::Failed { error, .. } => {
                assert_eq!(error, "Something went wrong");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn retrying_variant_contains_attempt_and_delay() {
        let event = JobEvent::Retrying {
            id: "job-1".to_string(),
            name: "Test Job".to_string(),
            attempt: 3,
            delay: Duration::from_secs(10),
        };
        
        match event {
            JobEvent::Retrying { attempt, delay, .. } => {
                assert_eq!(attempt, 3);
                assert_eq!(delay, Duration::from_secs(10));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn misfired_variant_contains_scheduled_time() {
        use chrono::Duration as ChronoDuration;
        
        let scheduled_time = Utc::now() - ChronoDuration::minutes(5);
        let event = JobEvent::Misfired {
            id: "job-1".to_string(),
            name: "Test Job".to_string(),
            scheduled_time,
        };
        
        match event {
            JobEvent::Misfired { scheduled_time: st, .. } => {
                assert_eq!(st, scheduled_time);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn event_is_cloneable() {
        let event1 = JobEvent::Started {
            id: "job-1".to_string(),
            name: "Test Job".to_string(),
        };
        
        let event2 = event1.clone();
        
        match (event1, event2) {
            (JobEvent::Started { id: id1, .. }, JobEvent::Started { id: id2, .. }) => {
                assert_eq!(id1, id2);
            }
            _ => panic!("Events should be equal"),
        }
    }
}

mod mock_event_listener {
    use super::*;
    use async_trait::async_trait;

    struct TestEventListener {
        event_count: Arc<AtomicUsize>,
        last_event: Arc<std::sync::Mutex<Option<JobEvent>>>,
    }

    impl TestEventListener {
        fn new() -> Self {
            Self {
                event_count: Arc::new(AtomicUsize::new(0)),
                last_event: Arc::new(std::sync::Mutex::new(None)),
            }
        }
    }

    #[async_trait]
    impl JobEventListener for TestEventListener {
        async fn on_event(&self, event: JobEvent) {
            self.event_count.fetch_add(1, Ordering::SeqCst);
            let mut last = self.last_event.lock().unwrap();
            *last = Some(event);
        }
    }

    #[tokio::test]
    async fn listener_receives_started_event() {
        let listener = Arc::new(TestEventListener::new());
        let event = JobEvent::Started {
            id: "job-1".to_string(),
            name: "Test Job".to_string(),
        };

        listener.on_event(event).await;

        assert_eq!(listener.event_count.load(Ordering::SeqCst), 1);
        let last = listener.last_event.lock().unwrap();
        assert!(matches!(last.as_ref().unwrap(), JobEvent::Started { .. }));
    }

    #[tokio::test]
    async fn listener_tracks_multiple_events() {
        let listener = Arc::new(TestEventListener::new());

        listener.on_event(JobEvent::Started { id: "job-1".to_string(), name: "Job".to_string() }).await;
        listener.on_event(JobEvent::Completed { id: "job-1".to_string(), name: "Job".to_string(), duration: Duration::from_secs(1) }).await;
        listener.on_event(JobEvent::Failed { id: "job-2".to_string(), name: "Job 2".to_string(), error: "error".to_string() }).await;

        assert_eq!(listener.event_count.load(Ordering::SeqCst), 3);
    }
}

mod mock_metrics_exporter {
    use super::*;
    use std::time::Duration;

    struct TestMetricsExporter {
        start_count: Arc<AtomicUsize>,
        completion_count: Arc<AtomicUsize>,
        failure_count: Arc<AtomicUsize>,
        retry_count: Arc<AtomicUsize>,
        misfire_count: Arc<AtomicUsize>,
    }

    impl TestMetricsExporter {
        fn new() -> Self {
            Self {
                start_count: Arc::new(AtomicUsize::new(0)),
                completion_count: Arc::new(AtomicUsize::new(0)),
                failure_count: Arc::new(AtomicUsize::new(0)),
                retry_count: Arc::new(AtomicUsize::new(0)),
                misfire_count: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl MetricsExporter for TestMetricsExporter {
        fn record_start(&self, _id: &str, _name: &str) {
            self.start_count.fetch_add(1, Ordering::SeqCst);
        }

        fn record_completion(&self, _id: &str, _name: &str, _duration: Duration) {
            self.completion_count.fetch_add(1, Ordering::SeqCst);
        }

        fn record_failure(&self, _id: &str, _name: &str) {
            self.failure_count.fetch_add(1, Ordering::SeqCst);
        }

        fn record_retry(&self, _id: &str, _name: &str) {
            self.retry_count.fetch_add(1, Ordering::SeqCst);
        }

        fn record_misfire(&self, _id: &str, _name: &str) {
            self.misfire_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn exporter_records_start() {
        let exporter = TestMetricsExporter::new();
        exporter.record_start("job-1", "Test Job");
        assert_eq!(exporter.start_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn exporter_records_completion_with_duration() {
        let exporter = TestMetricsExporter::new();
        exporter.record_completion("job-1", "Test Job", Duration::from_secs(5));
        assert_eq!(exporter.completion_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn exporter_records_failure() {
        let exporter = TestMetricsExporter::new();
        exporter.record_failure("job-1", "Test Job");
        assert_eq!(exporter.failure_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn exporter_records_retry() {
        let exporter = TestMetricsExporter::new();
        exporter.record_retry("job-1", "Test Job");
        assert_eq!(exporter.retry_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn exporter_records_misfire() {
        let exporter = TestMetricsExporter::new();
        exporter.record_misfire("job-1", "Test Job");
        assert_eq!(exporter.misfire_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn exporter_tracks_multiple_metrics() {
        let exporter = TestMetricsExporter::new();
        
        exporter.record_start("job-1", "Test Job");
        exporter.record_completion("job-1", "Test Job", Duration::from_secs(1));
        exporter.record_failure("job-2", "Test Job 2");
        exporter.record_retry("job-2", "Test Job 2");
        exporter.record_misfire("job-3", "Test Job 3");

        assert_eq!(exporter.start_count.load(Ordering::SeqCst), 1);
        assert_eq!(exporter.completion_count.load(Ordering::SeqCst), 1);
        assert_eq!(exporter.failure_count.load(Ordering::SeqCst), 1);
        assert_eq!(exporter.retry_count.load(Ordering::SeqCst), 1);
        assert_eq!(exporter.misfire_count.load(Ordering::SeqCst), 1);
    }
}
