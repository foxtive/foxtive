//! Resilient Backend Example - "Refuse to Die" Pattern
//!
//! This example demonstrates how to use the ResilientBackend wrapper
//! to make your message processing truly fault-tolerant. The backend
//! will automatically reconnect on network failures with exponential
//! backoff and jitter, retrying indefinitely until operations succeed.

use foxtive_worker::prelude::*;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> WorkerResult<()> {
    // Initialize tracing for better observability
    tracing_subscriber::fmt::init();

    println!("=== Resilient Backend Demo ===\n");

    // Example 1: RabbitMQ with resilient wrapper (default strategy)
    #[cfg(feature = "rabbitmq")]
    {
        println!("1. Creating RabbitMQ backend with automatic reconnection...");

        let rabbitmq = RabbitMqBackend::with_defaults("amqp://localhost")
            .await
            .expect("Failed to create RabbitMQ backend");

        // Wrap it in ResilientBackend - this adds automatic retry logic
        let resilient = Arc::new(ResilientBackend::new(Arc::new(rabbitmq)));

        println!("   ✓ Backend created");
        println!("   ✓ Will retry forever if connection drops");
        println!("   ✓ Exponential backoff: 1s → 2s → 4s → ... → 60s max\n");

        // Monitor connection state
        let resilient_monitor = resilient.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;

                let connected = resilient_monitor.is_connected();
                let attempts = resilient_monitor.reconnect_attempts();
                let failures = resilient_monitor.consecutive_failures();

                if !connected || attempts > 0 {
                    tracing::info!(
                        "Connection status: connected={}, attempts={}, failures={}",
                        connected,
                        attempts,
                        failures
                    );
                }
            }
        });

        // Now use the resilient backend normally
        // If connection drops, receive() will retry forever
        println!("Starting to consume messages (will survive network failures)...");

        loop {
            match resilient.receive().await {
                Ok(ReceiveResult::Message(msg)) => {
                    println!("Received message: {}", msg.message.id);
                    msg.ack().await?;
                }
                Ok(ReceiveResult::Shutdown) => {
                    // Backend shutting down
                    break;
                }
                Ok(other) => {
                    // Other statuses (Timeout, ConnectionLost, etc.)
                    // ResilientBackend handles retries internally
                    tracing::warn!("Unexpected status: {:?}", other);
                    continue;
                }
                Err(e) => {
                    // This branch is rarely reached because ResilientBackend retries internally
                    tracing::error!("Unexpected error: {}", e);
                    break;
                }
            }
        }
    }

    // Example 2: Redis Streams with custom reconnection strategy
    #[cfg(feature = "redis-stream")]
    {
        println!("\n2. Creating Redis backend with custom backoff strategy...");

        let redis = RedisStreamBackend::with_defaults("redis://localhost")
            .await
            .expect("Failed to create Redis backend");

        // Custom strategy: faster initial retries, more aggressive backoff
        let strategy = ReconnectStrategy::Exponential {
            initial: Duration::from_millis(500), // Start at 500ms
            max: Duration::from_secs(30),        // Max 30s delay
            multiplier: 2.5,                     // Grow faster
            jitter_factor: 0.15,                 // 15% jitter
        };

        let resilient = ResilientBackendBuilder::new(Arc::new(redis))
            .with_strategy(strategy)
            .build();

        println!("   ✓ Backend created with custom strategy");
        println!("   ✓ Backoff: 500ms → 1.25s → 3.1s → ... → 30s max");
        println!("   ✓ Jitter prevents thundering herd problem\n");

        // Use the resilient backend
        loop {
            if let ReceiveResult::Message(msg) = resilient.receive().await? {
                println!("Processing: {}", msg.message.id);
                msg.ack().await?;
            }
        }
    }

    // Example 3: Fixed delay strategy (for testing)
    #[cfg(not(any(feature = "rabbitmq", feature = "redis-stream")))]
    {
        println!("\n3. Using fixed delay strategy (good for testing)...");

        let memory_backend = Arc::new(MemoryBackend::new());
        let strategy = ReconnectStrategy::Fixed(Duration::from_secs(2));

        let resilient = ResilientBackend::with_strategy(memory_backend, strategy);

        println!("   ✓ Will retry every 2 seconds on failure");
        println!("   ✓ Predictable timing makes debugging easier\n");

        // Simulate operations that might fail
        loop {
            if let ReceiveResult::Message(msg) = resilient.receive().await? {
                println!("Got message: {:?}", msg.message.payload);
                msg.ack().await?;
            }
        }
    }

    #[allow(unreachable_code)]
    Ok(())
}
