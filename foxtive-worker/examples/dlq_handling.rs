//! Example demonstrating Dead Letter Queue (DLQ) and Poison Pill detection
//!
//! This example shows how to:
//! 1. Configure retry handling with exponential backoff
//! 2. Set up a Dead Letter Queue for permanently failed messages
//! 3. Detect poison pills (messages that consistently fail)
//! 4. Monitor DLQ metrics
//!
//! Run with: `cargo run --example dlq_handling --features rabbitmq`

use async_trait::async_trait;
use foxtive_worker::{Worker, ReceivedMessage};
use foxtive_worker::error::WorkerResult;

#[cfg(feature = "rabbitmq")]
use {
    foxtive_worker::backends::{RabbitMqBackend, RabbitMqConsumerConfig, DeadLetterQueueBackend},
    foxtive_worker::middleware::retry_handler::RetryHandlerConfig,
    foxtive_worker::dlq::{PoisonPillTracker, PoisonPillConfig},
    foxtive_worker::WorkerPoolBuilder,
    std::sync::Arc,
    std::time::Duration,
};

/// Worker that simulates processing failures
struct FailingWorker {
    id: String,
    fail_count: std::sync::atomic::AtomicUsize,
}

impl FailingWorker {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            fail_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl Worker for FailingWorker {
    fn id(&self) -> &str {
        &self.id
    }

    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        let count = self.fail_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        println!("Worker {} processing message: {}", self.id, message.message.id);
        println!("  Attempt: {}", message.message.metadata.attempt);
        println!("  Payload: {:?}", message.message.payload);
        
        // Simulate failures for first 3 messages, then succeed
        if count < 3 {
            println!("  ❌ Simulating failure (fail #{})", count + 1);
            Err(foxtive_worker::WorkerError::BackendError(
                format!("Simulated processing failure #{}", count + 1)
            ))
        } else {
            println!("  ✅ Processing succeeded!");
            Ok(())
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    #[cfg(not(feature = "rabbitmq"))]
    {
        println!("RabbitMQ feature not enabled. Run with: cargo run --example dlq_handling --features rabbitmq");
        return Ok(());
    }

    #[cfg(feature = "rabbitmq")]
    {
        println!("=== Dead Letter Queue & Poison Pill Detection Example ===\n");

        // 1. Create a DLQ backend (separate queue for failed messages)
        println!("Setting up Dead Letter Queue...");
        let dlq_config = RabbitMqConsumerConfig {
            queue_name: "dead_letter_queue".to_string(),
            consumer_tag: "dlq-consumer".to_string(),
            ..Default::default()
        };
        
        let dlq_backend = Arc::new(RabbitMqBackend::new("amqp://ahmard:Pass.1234@localhost", dlq_config).await?);
        let dlq_wrapper = Arc::new(DeadLetterQueueBackend::new(dlq_backend, "main-dlq"));
        println!("✅ DLQ configured: dead_letter_queue\n");

        // 2. Configure poison pill detection
        println!("Configuring poison pill detection...");
        let poison_config = PoisonPillConfig {
            max_failures: 5,  // Detect after 5 failures
            time_window: Duration::from_secs(3600), // Within 1 hour
            immediate_dlq: true,
        };
        let poison_tracker = Arc::new(PoisonPillTracker::new(poison_config.clone()));
        println!("✅ Poison pill detection: {} failures within 1 hour\n", poison_config.max_failures);

        // 3. Configure retry handler with DLQ and poison pill tracking
        println!("Setting up retry handler...");
        let retry_config = RetryHandlerConfig {
            max_retries: 5,
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            dead_letter_queue: Some(dlq_wrapper.clone()),
            poison_pill_tracker: Some(poison_tracker.clone()),
            use_jitter: true,
        };
        
        println!("✅ Retry handler configured:");
        println!("   - Max retries: {}", retry_config.max_retries);
        println!("   - Initial backoff: {:?}", retry_config.initial_backoff);
        println!("   - Max backoff: {:?}", retry_config.max_backoff);
        println!("   - DLQ enabled: yes");
        println!("   - Poison pill detection: yes\n");

        // 4. Create worker pool
        println!("Creating worker pool...\n");
        let worker = Arc::new(FailingWorker::new("failing-worker-1"));
        
        let pool = WorkerPoolBuilder::new("dlq-example-pool")
            .with_concurrency_limit(5)
            .add_arc_worker(worker)
            .build()?;

        println!("✅ Worker pool started\n");

        // 5. Send some test messages
        let backend_config = RabbitMqConsumerConfig {
            queue_name: "worker_queue".to_string(),
            consumer_tag: "test-producer".to_string(),
            ..Default::default()
        };
        
        let _backend = RabbitMqBackend::new("amqp://ahmard:Pass.1234@localhost", backend_config).await?;
        
        // Note: In a real scenario, you'd have a separate producer sending messages
        // For this example, we're just showing the configuration
        
        println!("Test messages would be sent here.");
        println!("The first 3 will fail, triggering retries.");
        println!("After 5 failed attempts, messages will be sent to DLQ.");
        println!("If a message fails 5 times within 1 hour, it's flagged as a poison pill.\n");

        // Keep running to show logs
        println!("Running for 30 seconds to observe behavior...");
        tokio::time::sleep(Duration::from_secs(30)).await;

        // Shutdown
        println!("\nShutting down...");
        pool.shutdown().await?;
        
        println!("\n=== Example Complete ===");
        println!("\nKey takeaways:");
        println!("1. Failed messages are automatically retried with exponential backoff");
        println!("2. After max retries, messages are sent to the Dead Letter Queue");
        println!("3. Poison pills (consistently failing messages) are detected and flagged");
        println!("4. All DLQ operations are logged and tracked via metrics");
    }

    Ok(())
}
