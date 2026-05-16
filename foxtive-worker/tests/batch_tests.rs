use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Mutex;

use foxtive_worker::{BatchConfig, BatchHandler, BatchProcessor, MessageBatch};
use foxtive_worker::error::WorkerResult;
use foxtive_worker::message::{Message, MessageMetadata, ReceivedMessage, AckHandle};

/// Mock ack handle for testing
#[derive(Debug)]
struct TestAckHandle;

#[async_trait]
impl AckHandle for TestAckHandle {
    async fn ack(&self) -> WorkerResult<()> { Ok(()) }
    async fn nack(&self, _requeue: bool) -> WorkerResult<()> { Ok(()) }
}

/// Test batch handler that tracks processed batches
struct TestBatchHandler {
    processed_batches: Arc<Mutex<Vec<MessageBatch<serde_json::Value>>>>,
}

impl TestBatchHandler {
    fn new() -> Self {
        Self {
            processed_batches: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn get_processed_count(&self) -> usize {
        self.processed_batches.lock().await.len()
    }
}

#[async_trait]
impl BatchHandler for TestBatchHandler {
    async fn process_batch(&self, batch: MessageBatch<serde_json::Value>) -> WorkerResult<()> {
        println!("Processing batch {} with {} messages", batch.id, batch.len());
        self.processed_batches.lock().await.push(batch);
        Ok(())
    }

    fn max_batch_size(&self) -> usize {
        5
    }

    fn max_batch_age(&self) -> Duration {
        Duration::from_secs(2)
    }
}

fn create_test_message(id: &str) -> ReceivedMessage<serde_json::Value> {
    ReceivedMessage::new(
        Message {
            id: id.to_string(),
            payload: serde_json::json!({"test": id}),
            metadata: MessageMetadata::new("test-queue"),
        },
        Arc::new(TestAckHandle),
    )
}

#[tokio::test]
async fn test_batch_processor_full_batch_flush() {
    // Create a test batch handler
    let handler = Arc::new(TestBatchHandler::new());
    
    // Configure batch processor with small batch size
    let config = BatchConfig::default()
        .with_batch_size(3)
        .with_flush_interval(Duration::from_secs(10))
        .wait_for_full_batch(true);
    
    let mut processor = BatchProcessor::new(handler.clone(), config);
    processor.start().await.expect("Failed to start processor");
    
    // Send exactly 3 messages (should trigger immediate flush)
    for i in 1..=3 {
        let msg = create_test_message(&format!("msg-{}", i));
        processor.enqueue(msg).await.expect("Failed to enqueue message");
    }
    
    // Wait for processing
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Verify one batch was processed
    let count = handler.get_processed_count().await;
    assert_eq!(count, 1, "Expected 1 batch to be processed");
    
    // Shutdown
    processor.shutdown().await.expect("Failed to shutdown");
}

#[tokio::test]
async fn test_batch_processor_partial_batch_timeout() {
    let handler = Arc::new(TestBatchHandler::new());
    
    let config = BatchConfig::default()
        .with_batch_size(10)
        .with_flush_interval(Duration::from_secs(1))
        .wait_for_full_batch(false);
    
    let mut processor = BatchProcessor::new(handler.clone(), config);
    processor.start().await.expect("Failed to start processor");
    
    // Send only 3 messages (less than batch_size)
    for i in 1..=3 {
        let msg = create_test_message(&format!("msg-{}", i));
        processor.enqueue(msg).await.expect("Failed to enqueue message");
    }
    
    // Wait for timeout flush (1 second + buffer)
    tokio::time::sleep(Duration::from_millis(1500)).await;
    
    // Verify the partial batch was flushed
    let count = handler.get_processed_count().await;
    assert_eq!(count, 1, "Expected 1 partial batch to be flushed");
    
    processor.shutdown().await.expect("Failed to shutdown");
}

#[tokio::test]
async fn test_batch_processor_multiple_batches() {
    let handler = Arc::new(TestBatchHandler::new());
    
    let config = BatchConfig::default()
        .with_batch_size(3)
        .with_flush_interval(Duration::from_secs(10))
        .wait_for_full_batch(true);
    
    let mut processor = BatchProcessor::new(handler.clone(), config);
    processor.start().await.expect("Failed to start processor");
    
    // Send 9 messages with small delays (should create 3 batches)
    for i in 1..=9 {
        let msg = create_test_message(&format!("msg-{}", i));
        processor.enqueue(msg).await.expect("Failed to enqueue message");
        // Small delay to allow batch processing between groups
        if i % 3 == 0 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    
    // Wait for processing
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Verify 3 batches were processed
    let count = handler.get_processed_count().await;
    assert_eq!(count, 3, "Expected 3 batches to be processed");
    
    processor.shutdown().await.expect("Failed to shutdown");
}

#[tokio::test]
async fn test_batch_processor_shutdown_flush() {
    let handler = Arc::new(TestBatchHandler::new());
    
    let config = BatchConfig::default()
        .with_batch_size(10)
        .with_flush_interval(Duration::from_secs(60))
        .wait_for_full_batch(true);
    
    let mut processor = BatchProcessor::new(handler.clone(), config);
    processor.start().await.expect("Failed to start processor");
    
    // Send 5 messages (less than batch_size)
    for i in 1..=5 {
        let msg = create_test_message(&format!("msg-{}", i));
        processor.enqueue(msg).await.expect("Failed to enqueue message");
    }
    
    // Immediately shutdown - should flush remaining
    tokio::time::sleep(Duration::from_millis(100)).await;
    processor.shutdown().await.expect("Failed to shutdown");
    
    // Give shutdown time to flush
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Verify the partial batch was flushed during shutdown
    let count = handler.get_processed_count().await;
    assert_eq!(count, 1, "Expected 1 batch to be flushed on shutdown");
}

#[tokio::test]
async fn test_batch_config_builder() {
    let config = BatchConfig::default()
        .with_batch_size(100)
        .with_flush_interval(Duration::from_secs(5))
        .wait_for_full_batch(true)
        .with_processing_timeout(Duration::from_secs(120));
    
    assert_eq!(config.batch_size, 100);
    assert_eq!(config.flush_interval, Duration::from_secs(5));
    assert!(config.wait_for_full_batch);
    assert_eq!(config.processing_timeout, Duration::from_secs(120));
}

#[tokio::test]
async fn test_batch_metadata() {
    use foxtive_worker::{ReceivedBatchMessage};
    
    let messages = vec![
        ReceivedBatchMessage::<serde_json::Value> {
            message: Message {
                id: "msg-1".to_string(),
                payload: serde_json::json!({"test": 1}),
                metadata: MessageMetadata::new("test"),
            },
            batch_index: 0,
        },
        ReceivedBatchMessage::<serde_json::Value> {
            message: Message {
                id: "msg-2".to_string(),
                payload: serde_json::json!({"test": 2}),
                metadata: MessageMetadata::new("test"),
            },
            batch_index: 1,
        },
    ];
    
    let batch = foxtive_worker::MessageBatch::new("test-batch".to_string(), messages);
    
    assert_eq!(batch.id, "test-batch");
    assert_eq!(batch.len(), 2);
    assert!(!batch.is_empty());
    assert!(batch.age() < Duration::from_secs(1));
}

#[tokio::test]
async fn test_batch_handler_defaults() {
    struct MinimalBatchHandler;
    
    #[async_trait]
    impl BatchHandler for MinimalBatchHandler {
        async fn process_batch(&self, _batch: MessageBatch<serde_json::Value>) -> WorkerResult<()> {
            Ok(())
        }
    }
    
    let handler = MinimalBatchHandler;
    assert_eq!(handler.max_batch_size(), 100);
    assert_eq!(handler.max_batch_age(), Duration::from_secs(30));
}
