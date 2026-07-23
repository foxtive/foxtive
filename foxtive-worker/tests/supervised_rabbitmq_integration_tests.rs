mod common;

use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

use common::create_test_message;
use foxtive_supervisor::contracts::SupervisedTask;
use foxtive_supervisor::SupervisorError;
use foxtive_worker::backends::{MemoryBackend, MessageBackend, ReceiveResult};
use foxtive_worker::error::WorkerResult;
use foxtive_worker::{AckNackMiddleware, ReceivedMessage, Worker, WorkerPool, WorkerPoolBuilder};

/// Wrapper that makes a WorkerPool compatible with foxtive-supervisor
struct SupervisedWorkerPool {
    name: &'static str,
    pool: Arc<WorkerPool>,
    backend: Arc<dyn MessageBackend>,
}

impl SupervisedWorkerPool {
    fn new(name: impl Into<String>, pool: WorkerPool, backend: Arc<dyn MessageBackend>) -> Self {
        // Leak the string once during construction to get a 'static reference
        let static_name = Box::leak(name.into().into_boxed_str());
        Self {
            name: static_name,
            pool: Arc::new(pool),
            backend,
        }
    }
}

#[async_trait]
impl SupervisedTask for SupervisedWorkerPool {
    fn id(&self) -> &'static str {
        self.name
    }

    async fn run(&self) -> foxtive_supervisor::SupervisorResult<()> {
        let backend = self.backend.clone();
        let pool = self.pool.clone();

        // Process messages from backend
        loop {
            match backend.receive().await.map_err(SupervisorError::wrap)? {
                ReceiveResult::Message(message) => {
                    if let Err(e) = pool.dispatch(*message).await {
                        eprintln!("Failed to dispatch: {}", e);
                    }
                }
                ReceiveResult::Shutdown => break,
                _ => {
                    // For other cases (connection lost, timeout, etc.), continue
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }

        Ok(())
    }

    async fn cleanup(&self) {
        let _ = self.pool.shutdown().await;
        let _ = self.backend.shutdown().await;
    }
}

/// Simple test worker
struct TestAppWorker {
    id: String,
}

#[async_trait]
impl Worker for TestAppWorker {
    fn id(&self) -> &str {
        &self.id
    }

    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        println!("Worker {} processing: {}", self.id, message.message.id);
        Ok(())
    }
}

/// Test basic supervised worker pool integration
#[tokio::test]
async fn test_supervised_worker_pool_basic() {
    // Create memory backend for testing
    let backend = Arc::new(MemoryBackend::new());

    // Create worker pool
    let worker = Arc::new(TestAppWorker {
        id: "test-worker-1".to_string(),
    });
    let pool = WorkerPoolBuilder::new("test-pool")
        .with_concurrency_limit(5)
        .with_middleware(AckNackMiddleware::default())
        .add_arc_worker(worker)
        .build()
        .unwrap();

    // Wrap for supervision
    let supervised = SupervisedWorkerPool::new("test-supervised-pool", pool, backend.clone());

    // Dispatch some test messages
    for i in 0..5 {
        let msg = create_test_message(&format!("msg-{}", i));
        backend.enqueue(msg.message.payload);
    }

    // Run processing in background
    let backend_clone = backend.clone();
    let handle = tokio::spawn(async move { supervised.run().await });

    // Wait for messages to be processed
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Trigger shutdown to stop the infinite loop
    backend_clone.shutdown().await.unwrap();

    // Wait for completion with timeout
    let result = tokio::time::timeout(Duration::from_secs(5), handle).await;

    assert!(result.is_ok(), "Test timed out");
    let inner_result = result.unwrap();
    assert!(inner_result.is_ok());
    assert!(inner_result.unwrap().is_ok());
}

/// Test supervised worker pool with multiple workers
#[tokio::test]
async fn test_supervised_worker_pool_multiple_workers() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let processed_count = Arc::new(AtomicUsize::new(0));

    struct CountingWorker {
        id: String,
        counter: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Worker for CountingWorker {
        fn id(&self) -> &str {
            &self.id
        }

        async fn process(&self, _message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
            self.counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    // Create backend
    let backend = Arc::new(MemoryBackend::new());

    // Create pool with multiple workers
    let mut builder = WorkerPoolBuilder::new("counting-pool")
        .with_concurrency_limit(10)
        .with_middleware(AckNackMiddleware::default());

    for i in 1..=3 {
        let worker = Arc::new(CountingWorker {
            id: format!("worker-{}", i),
            counter: processed_count.clone(),
        });
        builder = builder.add_arc_worker(worker);
    }

    let pool = builder.build().unwrap();

    // Wrap for supervision
    let supervised = SupervisedWorkerPool::new("counting-supervised-pool", pool, backend.clone());

    // Send messages
    for i in 0..10 {
        let msg = create_test_message(&format!("msg-{}", i));
        backend.enqueue(msg.message.payload);
    }

    // Run processing in background
    let backend_clone = backend.clone();
    let handle = tokio::spawn(async move { supervised.run().await });

    // Wait for messages to be processed
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Trigger shutdown
    backend_clone.shutdown().await.unwrap();

    // Wait for completion with timeout
    let result = tokio::time::timeout(Duration::from_secs(5), handle).await;

    assert!(result.is_ok(), "Test timed out");
    let inner_result = result.unwrap();
    assert!(inner_result.is_ok());
    assert!(inner_result.unwrap().is_ok());

    // Verify all messages were processed
    let count = processed_count.load(Ordering::SeqCst);
    assert_eq!(
        count, 10,
        "Expected 10 messages to be processed, got {}",
        count
    );
}

/// Test graceful shutdown of supervised worker pool
#[tokio::test]
async fn test_supervised_worker_pool_shutdown() {
    struct SlowWorker {
        id: String,
    }

    #[async_trait]
    impl Worker for SlowWorker {
        fn id(&self) -> &str {
            &self.id
        }

        async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
            // Simulate slow processing
            tokio::time::sleep(Duration::from_millis(100)).await;
            println!("Worker {} processed: {}", self.id, message.message.id);
            Ok(())
        }
    }

    let backend = Arc::new(MemoryBackend::new());

    let worker = Arc::new(SlowWorker {
        id: "slow-worker".to_string(),
    });
    let pool = WorkerPoolBuilder::new("slow-pool")
        .with_concurrency_limit(2)
        .with_middleware(AckNackMiddleware::default())
        .add_arc_worker(worker)
        .build()
        .unwrap();

    let supervised = SupervisedWorkerPool::new("slow-supervised-pool", pool, backend.clone());

    // Send messages
    for i in 0..3 {
        let msg = create_test_message(&format!("msg-{}", i));
        backend.enqueue(msg.message.payload);
    }

    // Start processing in background
    let handle = tokio::spawn(async move { supervised.run().await });

    // Let it process a bit
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Shutdown backend (should signal worker to stop)
    backend.shutdown().await.unwrap();

    // Wait for completion
    let result = handle.await.unwrap();
    assert!(result.is_ok());
}
