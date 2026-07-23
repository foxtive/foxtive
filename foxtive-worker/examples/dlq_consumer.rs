//! Dead Letter Queue Consumer Example
//!
//! This example demonstrates how to:
//! 1. Consume messages from a DLQ
//! 2. Inspect failure context
//! 3. Decide whether to retry, alert, or discard
//! 4. Implement manual retry logic
//!
//! Run with: `cargo run --example dlq_consumer`

use async_trait::async_trait;
use foxtive_worker::dlq::DeadLetterMessage;
use foxtive_worker::error::WorkerResult;
use foxtive_worker::{ReceivedMessage, Worker};

#[cfg(feature = "rabbitmq")]
use {
    foxtive_worker::WorkerPoolBuilder,
    foxtive_worker::backends::{RabbitMqBackend, RabbitMqConsumerConfig, ReceiveResult},
    std::sync::Arc,
};

/// DLQ Monitor Worker - Inspects and decides what to do with failed messages
#[allow(dead_code)]
struct DlqMonitorWorker {
    id: String,
}

#[async_trait]
impl Worker for DlqMonitorWorker {
    fn id(&self) -> &str {
        &self.id
    }

    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        println!("\n📬 DLQ Monitor received message: {}", message.message.id);

        // Parse the DLQ message format
        let payload_str = message.message.payload.to_string();

        match DeadLetterMessage::from_json(&payload_str) {
            Ok(dlq_msg) => {
                println!("  Original ID: {}", dlq_msg.original_id);
                println!("  Source Queue: {}", dlq_msg.source_queue);
                println!("  Attempts: {}", dlq_msg.attempt_count);
                println!("  Error: {}", dlq_msg.error_message);
                println!("  Timestamp: {}", dlq_msg.dlq_timestamp);

                if let Some(ref worker_id) = dlq_msg.last_worker_id {
                    println!("  Last Worker: {}", worker_id);
                }

                // Check if it's a poison pill
                if let serde_json::Value::Object(ref context) = dlq_msg.failure_context
                    && let Some(poison_pill) = context.get("poison_pill")
                    && poison_pill.as_bool() == Some(true)
                {
                    println!("  ⚠️  POISON PILL DETECTED!");
                    println!("  🚨 Action: Alert operations team, do NOT retry");

                    // In production: Send alert to Slack/PagerDuty
                    self.send_alert(&dlq_msg).await;

                    // Acknowledge - don't requeue
                    return Ok(());
                }

                // Decide what to do based on error type
                self.handle_dlq_message(&dlq_msg).await?;
            }
            Err(e) => {
                eprintln!("  ❌ Failed to parse DLQ message: {}", e);
                eprintln!("  Raw payload: {}", payload_str);
            }
        }

        Ok(())
    }
}

#[allow(dead_code)]
impl DlqMonitorWorker {
    /// Handle different types of DLQ messages
    async fn handle_dlq_message(&self, dlq_msg: &DeadLetterMessage) -> WorkerResult<()> {
        println!("\n  🤔 Deciding action for message...");

        // Extract error type from context
        let error_type = if let serde_json::Value::Object(ref context) = dlq_msg.failure_context {
            context
                .get("error_type")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
        } else {
            "Unknown"
        };

        match error_type {
            "RetriesExhausted" => {
                println!("  ℹ️  Message exhausted all retries");
                println!("  💡 Suggested action: Manual inspection required");

                // Option 1: Retry with different configuration
                // self.retry_with_backoff(dlq_msg).await?;

                // Option 2: Send to manual review queue
                // self.send_to_review_queue(dlq_msg).await?;

                // For now, just log
                println!("  ✅ Logged for manual review");
            }

            "RetryableFailure" => {
                println!("  ℹ️  Temporary failure occurred");
                println!("  💡 Suggested action: Retry after delay");

                // Could implement automatic retry here
                // self.schedule_retry(dlq_msg).await?;
            }

            _ => {
                println!("  ℹ️  Unknown error type");
                println!("  💡 Suggested action: Investigate and decide");
            }
        }

        Ok(())
    }

    /// Send alert for poison pills
    async fn send_alert(&self, dlq_msg: &DeadLetterMessage) {
        println!("\n  📧 Sending alert...");
        println!("     To: ops-team@company.com");
        println!(
            "     Subject: Poison Pill Detected - {}",
            dlq_msg.original_id
        );
        println!(
            "     Body: Message has failed {} times in source queue '{}'",
            dlq_msg.attempt_count, dlq_msg.source_queue
        );

        // In production:
        // - Send to Slack webhook
        // - Create PagerDuty incident
        // - Log to monitoring system
    }
}

#[tokio::main]
#[allow(unreachable_code)]
async fn main() -> WorkerResult<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    #[cfg(not(feature = "rabbitmq"))]
    {
        println!(
            "RabbitMQ feature not enabled. Run with: cargo run --example dlq_consumer --features rabbitmq"
        );
        return Ok(());
    }

    #[cfg(feature = "rabbitmq")]
    {
        use foxtive_worker::MessageBackend;

        println!("=== Dead Letter Queue Consumer Example ===\n");
        println!("This consumer monitors the DLQ and decides what to do with failed messages.\n");

        // Configure DLQ consumer
        let dlq_config = RabbitMqConsumerConfig {
            queue_name: "dead_letter_queue".to_string(),
            consumer_tag: "dlq-monitor".to_string(),
            prefetch_count: 5, // Process slowly to allow time for decisions
            ..Default::default()
        };

        println!("Connecting to DLQ: {}...", dlq_config.queue_name);
        let backend =
            Arc::new(RabbitMqBackend::new("amqp://ahmard:Pass.1234@localhost", dlq_config).await?);
        println!("✅ Connected to DLQ\n");

        // Create DLQ monitor worker
        let monitor = Arc::new(DlqMonitorWorker {
            id: "dlq-monitor-1".to_string(),
        });

        // Build worker pool
        let pool = Arc::new(
            WorkerPoolBuilder::new("dlq-consumer-pool")
                .with_concurrency_limit(3)
                .add_arc_worker(monitor)
                .build()?,
        );

        println!("DLQ Monitor started. Waiting for failed messages...\n");
        println!("Press Ctrl+C to stop.\n");

        // Spawn message receiver
        let backend_clone = backend.clone();
        let pool_clone = pool.clone();

        let receiver_handle = tokio::spawn(async move {
            loop {
                match backend_clone.receive().await {
                    Ok(ReceiveResult::Message(message)) => {
                        if let Err(e) = pool_clone.dispatch(*message).await {
                            eprintln!("Failed to dispatch DLQ message: {}", e);
                        }
                    }
                    Ok(ReceiveResult::Shutdown) => {
                        println!("Backend shutdown signal received");
                        break;
                    }
                    other => {
                        eprintln!("Unexpected receive status: {:?}", other);
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
            }
        });

        // Wait for shutdown
        tokio::signal::ctrl_c().await?;
        println!("\n\nShutting down DLQ monitor...");

        backend.shutdown().await?;
        let _ = receiver_handle.await;
        pool.shutdown().await?;

        println!("DLQ monitor stopped.");

        println!("\n=== Key Takeaways ===");
        println!("1. DLQ consumers inspect failure context before taking action");
        println!("2. Poison pills should trigger alerts, not retries");
        println!("3. Different error types may need different handling strategies");
        println!("4. Consider implementing retry queues for recoverable failures");
    }

    Ok(())
}
