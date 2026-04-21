mod common;
use foxtive_supervisor::Supervisor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

#[tokio::test]
async fn test_microservice_architecture() {
    struct DatabaseService {
        id: &'static str,
        initialized: Arc<AtomicBool>,
        query_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for DatabaseService {
        fn id(&self) -> &'static str {
            self.id
        }

        fn group_id(&self) -> Option<&'static str> {
            Some("infrastructure")
        }

        async fn setup(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            self.initialized.store(true, Ordering::SeqCst);
            Ok(())
        }

        async fn run(&self) -> anyhow::Result<()> {
            self.query_count.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(())
        }

        async fn health_check(&self) -> foxtive_supervisor::enums::HealthStatus {
            if self.initialized.load(Ordering::SeqCst) {
                foxtive_supervisor::enums::HealthStatus::Healthy
            } else {
                foxtive_supervisor::enums::HealthStatus::Unhealthy {
                    reason: "Database not initialized".to_string(),
                }
            }
        }
    }

    struct CacheService {
        id: &'static str,
        db_initialized: Arc<AtomicBool>,
        cache_hits: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for CacheService {
        fn id(&self) -> &'static str {
            self.id
        }

        fn group_id(&self) -> Option<&'static str> {
            Some("infrastructure")
        }

        fn dependencies(&self) -> &'static [&'static str] {
            &["database"]
        }

        async fn setup(&self) -> anyhow::Result<()> {
            while !self.db_initialized.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
            Ok(())
        }

        async fn run(&self) -> anyhow::Result<()> {
            self.cache_hits.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(80)).await;
            Ok(())
        }
    }

    struct ApiServer {
        id: &'static str,
        db_ready: Arc<AtomicBool>,
        cache_ready: Arc<AtomicBool>,
        requests_handled: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for ApiServer {
        fn id(&self) -> &'static str {
            self.id
        }

        fn group_id(&self) -> Option<&'static str> {
            Some("application")
        }

        fn dependencies(&self) -> &'static [&'static str] {
            &["database", "cache"]
        }

        async fn setup(&self) -> anyhow::Result<()> {
            while !self.db_ready.load(Ordering::SeqCst) || !self.cache_ready.load(Ordering::SeqCst)
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok(())
        }

        async fn run(&self) -> anyhow::Result<()> {
            self.requests_handled.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let db_initialized = Arc::new(AtomicBool::new(false));
    let cache_ready = Arc::new(AtomicBool::new(false));

    let db_queries = Arc::new(AtomicUsize::new(0));
    let cache_hits = Arc::new(AtomicUsize::new(0));
    let api_requests = Arc::new(AtomicUsize::new(0));

    let supervisor = Supervisor::new()
        .add(DatabaseService {
            id: "database",
            initialized: db_initialized.clone(),
            query_count: db_queries.clone(),
        })
        .add(CacheService {
            id: "cache",
            db_initialized: db_initialized.clone(),
            cache_hits: cache_hits.clone(),
        })
        .add(ApiServer {
            id: "api-server",
            db_ready: db_initialized.clone(),
            cache_ready: cache_ready.clone(),
            requests_handled: api_requests.clone(),
        });

    let runtime = supervisor.start().await.unwrap();

    // Mark services as ready immediately so API server can start
    tokio::time::sleep(Duration::from_millis(100)).await;
    db_initialized.store(true, Ordering::SeqCst);
    cache_ready.store(true, Ordering::SeqCst);

    // Give API server time to complete setup and run
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Verify infrastructure health
    let infra_health = runtime.get_group_health("infrastructure").await;
    match infra_health {
        foxtive_supervisor::enums::HealthStatus::Healthy => {}
        _ => panic!("Infrastructure should be healthy"),
    }

    assert!(db_queries.load(Ordering::SeqCst) > 0);
    assert!(cache_hits.load(Ordering::SeqCst) > 0);
    assert!(api_requests.load(Ordering::SeqCst) > 0);

    println!("✓ Microservice architecture passed");
    println!("  DB queries: {}", db_queries.load(Ordering::SeqCst));
    println!("  Cache hits: {}", cache_hits.load(Ordering::SeqCst));
    println!("  API requests: {}", api_requests.load(Ordering::SeqCst));

    runtime.shutdown().await;
}

#[tokio::test]
async fn test_message_processing_pipeline() {
    struct PipelineStage {
        id: &'static str,
        deps: &'static [&'static str],
        messages_processed: Arc<AtomicUsize>,
        processing_time_ms: u64,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for PipelineStage {
        fn id(&self) -> &'static str {
            self.id
        }

        fn dependencies(&self) -> &'static [&'static str] {
            self.deps
        }

        fn group_id(&self) -> Option<&'static str> {
            Some("pipeline")
        }

        async fn run(&self) -> anyhow::Result<()> {
            self.messages_processed.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(self.processing_time_ms)).await;
            Ok(())
        }
    }

    let ingest_count = Arc::new(AtomicUsize::new(0));
    let validate_count = Arc::new(AtomicUsize::new(0));
    let transform_count = Arc::new(AtomicUsize::new(0));
    let store_count = Arc::new(AtomicUsize::new(0));
    let notify_count = Arc::new(AtomicUsize::new(0));

    let supervisor = Supervisor::new()
        .add(PipelineStage {
            id: "ingest",
            deps: &[],
            messages_processed: ingest_count.clone(),
            processing_time_ms: 20,
        })
        .add(PipelineStage {
            id: "validate",
            deps: &["ingest"],
            messages_processed: validate_count.clone(),
            processing_time_ms: 30,
        })
        .add(PipelineStage {
            id: "transform",
            deps: &["validate"],
            messages_processed: transform_count.clone(),
            processing_time_ms: 40,
        })
        .add(PipelineStage {
            id: "store",
            deps: &["transform"],
            messages_processed: store_count.clone(),
            processing_time_ms: 50,
        })
        .add(PipelineStage {
            id: "notify",
            deps: &["store"],
            messages_processed: notify_count.clone(),
            processing_time_ms: 10,
        });

    let runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;

    let ingested = ingest_count.load(Ordering::SeqCst);
    let validated = validate_count.load(Ordering::SeqCst);
    let transformed = transform_count.load(Ordering::SeqCst);
    let stored = store_count.load(Ordering::SeqCst);
    let notified = notify_count.load(Ordering::SeqCst);

    assert!(ingested > 0);
    assert!(validated > 0);
    assert!(transformed > 0);
    assert!(stored > 0);
    assert!(notified > 0);

    println!("✓ Message pipeline passed");
    println!(
        "  Ingested: {}, Validated: {}, Transformed: {}, Stored: {}, Notified: {}",
        ingested, validated, transformed, stored, notified
    );

    runtime.shutdown().await;
}
