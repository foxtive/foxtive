//! Example demonstrating RabbitMQ backend usage with foxtive-worker
//!
//! This example shows how to:
//! 1. Create a RabbitMQ backend for receiving messages
//! 2. Build a worker pool to process messages
//! 3. Manually receive from backend and dispatch to pool
//!
//! Run with: `cargo run --example rabbitmq_backend --features rabbitmq`

use async_trait::async_trait;
use foxtive_worker::error::WorkerResult;
use foxtive_worker::{ReceivedMessage, Worker, WorkerPoolBuilder};

#[cfg(feature = "rabbitmq")]
use {
    foxtive_worker::backends::rabbitmq::{RabbitMqBackend, RabbitMqConsumerConfig},
    foxtive_worker::backends::{MessageBackend, ReceiveResult},
    std::sync::Arc,
};

/// Simple worker that processes messages
struct ExampleWorker {
    id: String,
}

#[async_trait]
impl Worker for ExampleWorker {
    fn id(&self) -> &str {
        &self.id
    }

    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        println!(
            "Worker {} processing message: {}",
            self.id, message.message.id
        );
        println!("Payload: {:?}", message.message.payload);

        // Process the message...
        // Your business logic here

        // NOTE: Don't manually call message.ack() - the pool handles acknowledgment automatically
        // after successful processing. Only call ack/nack if you're using a custom middleware chain.

        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    #[cfg(not(feature = "rabbitmq"))]
    {
        println!(
            "RabbitMQ feature not enabled. Run with: cargo run --example rabbitmq_backend --features rabbitmq"
        );
        return Ok(());
    }

    #[cfg(feature = "rabbitmq")]
    {
        // Configure RabbitMQ consumer
        let config = RabbitMqConsumerConfig {
            queue_name: "worker_queue".to_string(),
            consumer_tag: "example-worker".to_string(),
            prefetch_count: 10,
            ..Default::default()
        };

        // Create RabbitMQ backend
        println!("Connecting to RabbitMQ...");
        let backend =
            Arc::new(RabbitMqBackend::new("amqp://ahmard:Pass.1234@localhost", config).await?);
        println!("Connected to RabbitMQ!");

        // Create workers
        let worker1 = Arc::new(ExampleWorker {
            id: "worker-1".to_string(),
        });
        let worker2 = Arc::new(ExampleWorker {
            id: "worker-2".to_string(),
        });
        let worker3 = Arc::new(ExampleWorker {
            id: "worker-3".to_string(),
        });

        // Build worker pool
        let pool = Arc::new(
            WorkerPoolBuilder::new("rabbitmq-pool")
                .with_concurrency_limit(10)
                .add_arc_worker(worker1)
                .add_arc_worker(worker2)
                .add_arc_worker(worker3)
                .build()?,
        );

        println!("Worker pool started. Waiting for messages...");
        println!("Press Ctrl+C to stop.");

        // Spawn message receiver task
        let backend_clone = backend.clone();
        let pool_clone = pool.clone();

        let receiver_handle = tokio::spawn(async move {
            loop {
                match backend_clone.receive().await {
                    Ok(ReceiveResult::Message(message)) => {
                        // Dispatch message to pool for processing
                        if let Err(e) = pool_clone.dispatch(message).await {
                            eprintln!("Failed to dispatch message: {}", e);
                        }
                    }
                    Ok(ReceiveResult::Shutdown) => {
                        // Backend shutdown
                        println!("Backend received shutdown signal");
                        break;
                    }
                    other => {
                        eprintln!("Unexpected receive status: {:?}", other);
                        // Continue trying
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        });

        // Wait for shutdown signal
        tokio::signal::ctrl_c().await?;
        println!("\nShutting down...");

        // Shutdown backend first (stops receiving new messages)
        backend.shutdown().await?;

        // Wait for receiver to finish
        let _ = receiver_handle.await;

        // Then shutdown pool (waits for in-flight messages)
        pool.shutdown().await?;
        println!("Shutdown complete.");
    }

    Ok(())
}
