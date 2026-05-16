//! Example: Using Redis Streams with Foxtive Worker
//!
//! This example demonstrates how to set up a worker pool using Redis Streams,
//! including how to expose a health check endpoint for Kubernetes.
//!
//! To run this example:
//! `cargo run --example redis_streams_backend --features redis-stream,http`

use foxtive_worker::{Worker, ReceivedMessage, WorkerPoolBuilder};
use foxtive_worker::error::WorkerResult;
use foxtive_worker::http::HealthEndpoint;
use foxtive_worker::backends::{MessageBackend, ReceiveResult};
use async_trait::async_trait;
use std::sync::Arc;

struct RedisWorker;

#[async_trait]
impl Worker for RedisWorker {
    fn id(&self) -> &str { "redis-worker" }

    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        println!("Received from Redis Stream: {:?}", message.message.payload);
        
        // Simulate processing
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create the Redis Streams backend
    #[cfg(feature = "redis-stream")]
    {
        let config = foxtive_worker::backends::redis_stream::RedisStreamConsumerConfig {
            stream_name: "my_tasks".to_string(),
            group_name: "worker_group".to_string(),
            consumer_name: "consumer_1".to_string(),
            ..Default::default()
        };

        let backend = Arc::new(
            foxtive_worker::backends::RedisStreamBackend::new("redis://localhost", config).await?
        );

        // Build the worker pool
        let pool = Arc::new(
            WorkerPoolBuilder::new("redis-pool")
                .add_worker(RedisWorker)
                .build()?
        );

        // Expose health endpoint (simulated here)
        let health = HealthEndpoint::new(pool.clone());
        println!("Health Status: {}", health.get_status_json());

        // In a real app, you would integrate `health` into your Axum/Actix router here.

        // Manually receive from backend and dispatch to pool
        let backend_clone = backend.clone();
        let pool_clone = pool.clone();
        
        let receiver_handle = tokio::spawn(async move {
            loop {
                match backend_clone.receive().await {
                    Ok(ReceiveResult::Message(message)) => {
                        if let Err(e) = pool_clone.dispatch(message).await {
                            eprintln!("Failed to dispatch message: {}", e);
                        }
                    }
                    Ok(ReceiveResult::Shutdown) => {
                        println!("Backend shutdown signal received");
                        break;
                    }
                    other => {
                        eprintln!("Unexpected receive status: {:?}", other);
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        });

        // Wait for shutdown signal
        tokio::signal::ctrl_c().await?;
        println!("\nShutting down...");
        
        backend.shutdown().await?;
        let _ = receiver_handle.await;
        pool.shutdown().await?;
    }

    Ok(())
}
