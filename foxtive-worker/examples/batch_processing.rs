//! Batch Processing Example
//!
//! This example demonstrates how to:
//! 1. Implement a BatchHandler for processing messages in batches
//! 2. Configure batch size and flush intervals
//! 3. Use BatchProcessor for automatic batch assembly
//! 4. Handle batch failures and retries
//!
//! Run with: `cargo run --example batch_processing`

use async_trait::async_trait;
use foxtive_worker::{BatchHandler, BatchConfig, MessageBatch, BatchProcessor};
use std::sync::Arc;
use std::time::Duration;

/// Example batch handler that simulates bulk database writes
struct DatabaseBatchHandler;

#[async_trait]
impl BatchHandler for DatabaseBatchHandler {
    async fn process_batch(&self, batch: MessageBatch<serde_json::Value>) -> Result<(), foxtive_worker::WorkerError> {
        println!("\n📦 Processing batch: {}", batch.id);
        println!("   Messages: {}", batch.len());
        println!("   Age: {:?}", batch.age());
        
        // Simulate bulk database operation
        println!("   Performing bulk insert...");
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Process each message in the batch
        for (idx, msg) in batch.messages.iter().enumerate() {
            println!("     [{}] Message {}: {:?}", idx + 1, msg.message.id, msg.message.payload);
        }
        
        println!("   ✅ Batch completed successfully");
        
        Ok(())
    }
    
    fn max_batch_size(&self) -> usize {
        10 // Smaller batch size for demo
    }
    
    fn max_batch_age(&self) -> Duration {
        Duration::from_secs(5) // Flush every 5 seconds
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("=== Batch Processing Example ===\n");
    
    // 1. Create a batch handler
    println!("Creating batch handler...");
    let handler = Arc::new(DatabaseBatchHandler);
    println!("✅ Batch handler created\n");
    
    // 2. Configure batch processing
    println!("Configuring batch processor...");
    let config = BatchConfig::default()
        .with_batch_size(5) // Small batch size for demo
        .with_flush_interval(Duration::from_secs(3)) // Flush every 3 seconds
        .wait_for_full_batch(false) // Don't wait - flush on timeout too
        .with_processing_timeout(Duration::from_secs(30));
    
    println!("   Batch size: {}", config.batch_size);
    println!("   Flush interval: {:?}", config.flush_interval);
    println!("   Wait for full batch: {}", config.wait_for_full_batch);
    println!("   Processing timeout: {:?}", config.processing_timeout);
    println!("✅ Configuration complete\n");
    
    // 3. Create and start batch processor
    println!("Starting batch processor...");
    let mut processor = BatchProcessor::new(handler, config);
    processor.start().await?;
    println!("✅ Batch processor started\n");
    
    println!("Simulating message arrival...\n");
    
    // 4. Simulate messages arriving
    use foxtive_worker::{Message, MessageMetadata, ReceivedMessage};
    
    // Create a mock ack handle
    #[derive(Debug)]
    struct MockAckHandle;
    #[async_trait]
    impl foxtive_worker::message::AckHandle for MockAckHandle {
        async fn ack(&self) -> Result<(), foxtive_worker::WorkerError> { Ok(()) }
        async fn nack(&self, _requeue: bool) -> Result<(), foxtive_worker::WorkerError> { Ok(()) }
    }
    
    // Send 12 messages (should trigger 2 full batches + 1 partial)
    for i in 1..=12 {
        let message = ReceivedMessage::new(
            Message {
                id: format!("msg-{}", i),
                payload: serde_json::json!({
                    "user_id": i,
                    "action": "purchase",
                    "amount": i * 10
                }),
                metadata: MessageMetadata::new("orders"),
            },
            Arc::new(MockAckHandle),
        );
        
        println!("📨 Enqueuing message {}", i);
        processor.enqueue(message).await?;
        
        // Add some delay to show batching behavior
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    
    println!("\n⏳ Waiting for final batch to flush...");
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // 5. Shutdown
    println!("\n🛑 Shutting down batch processor...");
    processor.shutdown().await?;
    println!("✅ Shutdown complete");
    
    println!("\n=== Example Complete ===");
    println!("\nKey takeaways:");
    println!("1. Messages are automatically assembled into batches");
    println!("2. Batches flush when they reach batch_size OR flush_interval");
    println!("3. All messages in a batch are processed together");
    println!("4. Partial batches are flushed on timeout or shutdown");
    println!("5. Batch processing is more efficient for bulk operations");
    
    Ok(())
}
