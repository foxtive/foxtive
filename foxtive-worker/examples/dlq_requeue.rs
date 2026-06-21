//! DLQ Reprocessing Example
//!
//! This example demonstrates how to use the DlqManager to requeue failed messages
//! from the Dead Letter Queue back to the main queue for reprocessing.
//!
//! Key features demonstrated:
//! 1. Creating a DlqManager with DLQ and main backends
//! 2. Configuring retry filters (e.g., skip poison pills)
//! 3. Reprocessing individual messages
//! 4. Bulk reprocessing all DLQ messages
//!
//! Run with: `cargo run --example dlq_requeue --features rabbitmq`

use async_trait::async_trait;
use foxtive_worker::dlq::{DeadLetterMessage, DlqManager};
use foxtive_worker::error::WorkerResult;
use foxtive_worker::{ReceivedMessage, Worker};

#[cfg(feature = "rabbitmq")]
use {
    foxtive_worker::backends::{RabbitMqBackend, RabbitMqConsumerConfig},
    std::sync::Arc,
    std::time::Duration,
};

/// Worker that processes messages from the DLQ and decides whether to requeue them
struct DlqRequeueWorker {
    id: String,
    manager: Arc<DlqManager>,
}

#[async_trait]
impl Worker for DlqRequeueWorker {
    fn id(&self) -> &str {
        &self.id
    }

    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        println!(
            "\n📬 DLQ Requeue Worker received message: {}",
            message.message.id
        );

        // Use the DlqManager to handle requeuing logic
        match self.manager.reprocess_single(&message).await {
            Ok(true) => {
                println!("  ✅ Message successfully requeued to source queue");
            }
            Ok(false) => {
                println!("  ⏭️  Message skipped (filtered out or malformed)");
            }
            Err(e) => {
                eprintln!("  ❌ Failed to requeue message: {}", e);
                // Don't ack - let it stay in DLQ for manual inspection
                return Err(e);
            }
        }

        Ok(())
    }
}

/// Custom filter function that determines which messages should be retried
fn smart_retry_filter(dlq_msg: &DeadLetterMessage) -> bool {
    println!(
        "  🔍 Evaluating message {} for retry...",
        dlq_msg.original_id
    );

    // Check if it's a poison pill
    if let serde_json::Value::Object(ref context) = dlq_msg.failure_context {
        if let Some(poison_pill) = context.get("poison_pill")
            && poison_pill.as_bool() == Some(true)
        {
            println!(
                "  🚫 Skipping poison pill (failed {} times)",
                dlq_msg.attempt_count
            );
            return false;
        }

        // Check error type
        if let Some(error_type) = context.get("error_type").and_then(|v| v.as_str()) {
            match error_type {
                "RetriesExhausted" => {
                    println!("  ⚠️  Retries exhausted - will retry with fresh attempt counter");
                    // Allow retry but could implement additional logic here
                }
                "RetryableFailure" => {
                    println!("  ✓ Temporary failure - safe to retry");
                }
                _ => {
                    println!("  ❓ Unknown error type - proceeding with caution");
                }
            }
        }
    }

    // Additional custom logic could go here:
    // - Check message age (don't retry very old messages)
    // - Check payload content
    // - Check time of day (batch processing windows)
    // - Rate limiting

    true
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    #[cfg(not(feature = "rabbitmq"))]
    {
        println!(
            "RabbitMQ feature not enabled. Run with: cargo run --example dlq_requeue --features rabbitmq"
        );
        return Ok(());
    }

    #[cfg(feature = "rabbitmq")]
    {
        println!("=== DLQ Reprocessing Example ===\n");

        // 1. Set up the main queue backend
        println!("Setting up main queue backend...");
        let main_config = RabbitMqConsumerConfig {
            queue_name: "orders".to_string(),
            consumer_tag: "main-consumer".to_string(),
            ..Default::default()
        };
        let main_backend =
            Arc::new(RabbitMqBackend::new("amqp://ahmard:Pass.1234@localhost", main_config).await?);
        println!("✅ Main queue configured: orders\n");

        // 2. Set up the DLQ backend
        println!("Setting up DLQ backend...");
        let dlq_config = RabbitMqConsumerConfig {
            queue_name: "orders-dlq".to_string(),
            consumer_tag: "dlq-requeue-consumer".to_string(),
            prefetch_count: 5, // Process slowly to allow inspection
            ..Default::default()
        };
        let dlq_backend =
            Arc::new(RabbitMqBackend::new("amqp://ahmard:Pass.1234@localhost", dlq_config).await?);
        println!("✅ DLQ configured: orders-dlq\n");

        // 3. Create DlqManager with smart retry filter
        println!("Creating DlqManager with custom retry filter...");
        let manager = Arc::new(
            DlqManager::new(dlq_backend.clone(), main_backend.clone())
                .with_retry_filter(smart_retry_filter),
        );
        println!("✅ DlqManager ready\n");

        // 4. Option 1: Manual bulk reprocessing
        println!("Option 1: Bulk reprocess all DLQ messages");
        println!("-------------------------------------------");
        match manager.reprocess_all().await {
            Ok(count) => {
                println!("✅ Successfully requeued {} messages from DLQ", count);
            }
            Err(e) => {
                eprintln!("❌ Error during bulk reprocessing: {}", e);
            }
        }

        println!("\nOption 2: Starting interactive DLQ monitor...");
        println!("This worker will process DLQ messages as they arrive.\n");

        let requeue_worker = Arc::new(DlqRequeueWorker {
            id: "dlq-requeue-worker-1".to_string(),
            manager: manager.clone(),
        });

        // Build worker pool for DLQ processing
        use foxtive_worker::WorkerPoolBuilder;
        let pool = Arc::new(
            WorkerPoolBuilder::new("dlq-requeue-pool")
                .with_concurrency_limit(3)
                .add_arc_worker(requeue_worker)
                .build()?,
        );

        println!("DLQ Requeue Worker started. Waiting for messages...\n");
        println!("Send test messages to 'orders-dlq' to see them processed.\n");
        println!("The worker will:");
        println!("  1. Parse each DLQ message");
        println!("  2. Apply the retry filter");
        println!("  3. Requeue approved messages to the main queue");
        println!("  4. Skip poison pills and malformed messages\n");

        // Keep running to process messages
        println!("Running for 60 seconds...");
        tokio::time::sleep(Duration::from_secs(60)).await;

        // Shutdown
        println!("\nShutting down...");
        pool.shutdown().await?;

        println!("\n=== Example Complete ===");
        println!("\nKey takeaways:");
        println!("1. DlqManager simplifies DLQ message reprocessing");
        println!("2. Custom filters allow fine-grained control over which messages to retry");
        println!("3. Poison pills can be automatically detected and skipped");
        println!("4. Both bulk and incremental reprocessing are supported");
        println!("5. All operations are logged for audit and debugging");
    }

    Ok(())
}
