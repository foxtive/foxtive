//! Worker should_requeue Example
//!
//! This example demonstrates how to implement the `should_requeue` method
//! in your worker to control whether failed messages should be retried or
//! sent directly to the Dead Letter Queue.
//!
//! Run with: `cargo run --example worker_should_requeue`

use async_trait::async_trait;
use foxtive_worker::error::{RetryInfo, WorkerError, WorkerResult};
use foxtive_worker::{ReceivedMessage, Worker};
use std::sync::Arc;

/// A smart worker that decides whether to retry based on error type and message content
struct SmartOrderProcessor;

#[async_trait]
impl Worker for SmartOrderProcessor {
    fn id(&self) -> &str {
        "smart-order-processor"
    }

    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        println!("Processing order: {}", message.message.id);

        // Simulate different types of failures based on payload
        if let Some(order_type) = message
            .message
            .payload
            .get("order_type")
            .and_then(|v| v.as_str())
        {
            match order_type {
                "invalid" => {
                    return Err(WorkerError::ProcessingError(
                        "Invalid order type - validation failed".to_string(),
                    ));
                }
                "temp_error" => {
                    return Err(WorkerError::ProcessingError(
                        "Temporary database connection error".to_string(),
                    ));
                }
                "malformed" => {
                    // Missing required fields
                    return Err(WorkerError::ProcessingError(
                        "Malformed order data".to_string(),
                    ));
                }
                _ => {
                    println!("✅ Order processed successfully");
                    message.ack().await?;
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    /// Custom logic to decide whether failed messages should be requeued
    fn should_requeue(
        &self,
        message: &ReceivedMessage<serde_json::Value>,
        info: RetryInfo<'_>,
    ) -> bool {
        println!(
            "\n🤔 Evaluating whether to requeue message {}...",
            message.message.id
        );
        println!("   Error: {:?}", info.error);

        // Rule 1: Don't retry validation errors - they won't succeed on retry
        if let WorkerError::ProcessingError(msg) = info.error
            && msg.contains("validation failed")
        {
            println!("   ❌ Validation error - sending to DLQ (won't retry)");
            return false;
        }

        // Rule 2: Don't retry malformed messages - missing required fields won't be fixed by retrying
        if let Some(payload) = message.message.payload.as_object()
            && (!payload.contains_key("customer_id") || !payload.contains_key("amount"))
        {
            println!("   ❌ Malformed message (missing required fields) - sending to DLQ");
            return false;
        }

        // Rule 3: Retry temporary/transient errors
        if let WorkerError::ProcessingError(msg) = info.error
            && (msg.contains("Temporary") || msg.contains("connection"))
        {
            println!("   ✅ Temporary error - will retry");
            return true;
        }

        // Rule 4: Check message age - don't retry very old messages
        if let Some(timestamp) = message
            .message
            .payload
            .get("created_at")
            .and_then(|v| v.as_i64())
        {
            let now = chrono::Utc::now().timestamp();
            let age_seconds = now - timestamp;
            if age_seconds > 3600 {
                // Older than 1 hour
                println!(
                    "   ❌ Message too old ({} seconds) - sending to DLQ",
                    age_seconds
                );
                return false;
            }
        }

        // Default: retry unknown errors
        println!("   ✅ Unknown error type - will retry as fallback");
        true
    }
}

/// Example worker that never retries certain business logic failures
struct PaymentProcessor;

#[async_trait]
impl Worker for PaymentProcessor {
    fn id(&self) -> &str {
        "payment-processor"
    }

    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        println!("Processing payment: {}", message.message.id);

        // Simulate payment processing
        if let Some(amount) = message
            .message
            .payload
            .get("amount")
            .and_then(|v| v.as_f64())
        {
            if amount <= 0.0 {
                return Err(WorkerError::ProcessingError(
                    "Invalid payment amount".to_string(),
                ));
            }

            if amount > 10000.0 {
                return Err(WorkerError::ProcessingError(
                    "Payment exceeds limit - requires manual approval".to_string(),
                ));
            }

            println!("✅ Payment of ${:.2} processed", amount);
            message.ack().await?;
            return Ok(());
        }

        Err(WorkerError::ProcessingError(
            "Missing payment amount".to_string(),
        ))
    }

    fn should_requeue(
        &self,
        _message: &ReceivedMessage<serde_json::Value>,
        info: RetryInfo<'_>,
    ) -> bool {
        println!("\n💳 Payment processor evaluating retry...");

        // Never retry invalid amounts - the data is wrong
        if let WorkerError::ProcessingError(msg) = info.error {
            if msg.contains("Invalid payment amount") || msg.contains("Missing payment") {
                println!("   ❌ Invalid payment data - send to DLQ for review");
                return false;
            }

            // Don't retry payments that exceed limits - needs human intervention
            if msg.contains("exceeds limit") {
                println!("   ❌ Exceeds limit - send to manual approval queue");
                return false;
            }
        }

        // For other errors (network issues, etc.), retry
        println!("   ✅ Will retry payment processing");
        true
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    println!("=== Worker should_requeue Example ===\n");
    println!("This example shows how workers can control retry behavior.\n");

    // Create instances of our smart workers
    let order_processor = Arc::new(SmartOrderProcessor);
    let payment_processor = Arc::new(PaymentProcessor);

    println!("Workers created:");
    println!("  1. SmartOrderProcessor - evaluates order errors");
    println!("  2. PaymentProcessor - controls payment retry logic\n");

    println!("Key scenarios demonstrated:\n");

    println!("Scenario 1: Validation Errors");
    println!("  - Invalid order type → Don't retry (send to DLQ)");
    println!("  - Reason: Data validation won't pass on retry\n");

    println!("Scenario 2: Malformed Messages");
    println!("  - Missing required fields → Don't retry (send to DLQ)");
    println!("  - Reason: Structure is broken, retrying won't fix it\n");

    println!("Scenario 3: Temporary Failures");
    println!("  - Database connection errors → Retry");
    println!("  - Reason: Transient issues often resolve quickly\n");

    println!("Scenario 4: Business Logic Failures");
    println!("  - Invalid payment amounts → Don't retry (send to DLQ)");
    println!("  - Exceeds payment limits → Don't retry (needs manual approval)");
    println!("  - Reason: These require human intervention\n");

    println!("Scenario 5: Age-Based Decisions");
    println!("  - Messages older than 1 hour → Don't retry");
    println!("  - Reason: Old messages may no longer be relevant\n");

    println!("\nImplementation Tips:");
    println!("  ✓ Check error types and messages for patterns");
    println!("  ✓ Inspect message payload for validity");
    println!("  ✓ Consider message age and business context");
    println!("  ✓ Use should_requeue to prevent infinite retry loops");
    println!("  ✓ Send problematic messages to DLQ for manual inspection\n");

    println!("To see this in action, integrate these workers into a WorkerPool");
    println!("with a message backend like RabbitMQ or Redis Streams.\n");

    // Keep references to avoid unused warnings
    let _ = (order_processor, payment_processor);

    println!("Example complete!");

    Ok(())
}
