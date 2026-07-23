//! Example demonstrating processing timeout middleware and per-worker timeout configuration.
//!
//! This example shows how to:
//! 1. Use `ProcessingTimeoutMiddleware` to enforce maximum processing times
//! 2. Configure different timeouts for different workers
//! 3. Prevent RabbitMQ consumer timeout errors
//!
//! Run with: `cargo run --example processing_timeout`

use async_trait::async_trait;
use foxtive_worker::{
    ProcessingTimeoutMiddleware, ReceivedMessage, Worker, WorkerPoolBuilder, error::WorkerResult,
};
use std::time::Duration;

/// Fast worker - processes messages quickly (under 1 second)
struct FastWorker {
    id: String,
}

#[async_trait]
impl Worker for FastWorker {
    fn id(&self) -> &str {
        &self.id
    }

    // Fast workers can have a short timeout
    fn processing_timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(5))
    }

    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        println!(
            "[{}] Processing fast message: {}",
            self.id, message.message.id
        );

        // Simulate fast processing
        tokio::time::sleep(Duration::from_millis(500)).await;

        println!(
            "[{}] Completed fast message: {}",
            self.id, message.message.id
        );
        message.ack().await?;
        Ok(())
    }
}

/// Slow worker - legitimately takes longer (e.g., complex computations, external API calls)
struct SlowWorker {
    id: String,
}

#[async_trait]
impl Worker for SlowWorker {
    fn id(&self) -> &str {
        &self.id
    }

    // Slow workers need longer timeouts
    // Set this to LESS than your broker's consumer timeout
    // If RabbitMQ has 30s timeout, use 25s here
    fn processing_timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(25))
    }

    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        println!(
            "[{}] Processing slow message: {}",
            self.id, message.message.id
        );

        // Simulate slow processing (e.g., calling external APIs, heavy computation)
        tokio::time::sleep(Duration::from_secs(3)).await;

        println!(
            "[{}] Completed slow message: {}",
            self.id, message.message.id
        );
        message.ack().await?;
        Ok(())
    }
}

/// Very slow worker - might exceed default timeouts
struct VerySlowWorker {
    id: String,
}

#[async_trait]
impl Worker for VerySlowWorker {
    fn id(&self) -> &str {
        &self.id
    }

    // For very long tasks, configure appropriate timeout
    // This prevents RabbitMQ from killing the connection at 30s
    fn processing_timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(120)) // 2 minutes
    }

    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        println!(
            "[{}] Processing very slow message: {}",
            self.id, message.message.id
        );

        // Simulate very slow processing
        tokio::time::sleep(Duration::from_secs(5)).await;

        println!(
            "[{}] Completed very slow message: {}",
            self.id, message.message.id
        );
        message.ack().await?;
        Ok(())
    }
}

/// Worker that intentionally times out (for demonstration)
struct TimeoutWorker {
    id: String,
}

#[async_trait]
impl Worker for TimeoutWorker {
    fn id(&self) -> &str {
        &self.id
    }

    // Short timeout to demonstrate timeout handling
    fn processing_timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(2))
    }

    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        println!(
            "[{}] Starting message that will timeout: {}",
            self.id, message.message.id
        );

        // This will exceed the 2-second timeout
        tokio::time::sleep(Duration::from_secs(10)).await;

        // This line won't be reached due to timeout
        message.ack().await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> WorkerResult<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("=== Processing Timeout Example ===\n");

    // Create workers with different timeout configurations
    let fast_worker_1 = std::sync::Arc::new(FastWorker {
        id: "fast-1".to_string(),
    });
    let fast_worker_2 = std::sync::Arc::new(FastWorker {
        id: "fast-2".to_string(),
    });

    let slow_worker = std::sync::Arc::new(SlowWorker {
        id: "slow-1".to_string(),
    });

    let very_slow_worker = std::sync::Arc::new(VerySlowWorker {
        id: "very-slow-1".to_string(),
    });

    let timeout_worker = std::sync::Arc::new(TimeoutWorker {
        id: "timeout-1".to_string(),
    });

    println!("Workers configured:");
    println!("  - Fast workers: 5s timeout each");
    println!("  - Slow worker: 25s timeout");
    println!("  - Very slow worker: 120s timeout");
    println!("  - Timeout worker: 2s timeout (will trigger timeout)\n");

    // Build worker pool with processing timeout middleware
    // The middleware provides a safety net for all workers
    println!("Building worker pool with ProcessingTimeoutMiddleware...\n");

    let _pool = WorkerPoolBuilder::new("timeout-example-pool")
        .with_concurrency_limit(5)
        .add_arc_worker(fast_worker_1)
        .add_arc_worker(fast_worker_2)
        .add_arc_worker(slow_worker)
        .add_arc_worker(very_slow_worker)
        .add_arc_worker(timeout_worker)
        // Add global timeout middleware as safety net
        // This catches any worker that doesn't set its own timeout
        .with_middleware(ProcessingTimeoutMiddleware::new(Duration::from_secs(30)))
        .build()?;

    println!("✅ Worker pool created successfully\n");

    // Note: In a real application, you would connect this to a message backend
    // like RabbitMQ or Redis Streams. For this example, we're just showing
    // the configuration.

    println!("To use with RabbitMQ:");
    println!(
        "  1. Start RabbitMQ: docker run -d --name rabbitmq -p 5672:5672 rabbitmq:3-management"
    );
    println!("  2. Configure RabbitMqBackend with your queue");
    println!("  3. Connect the pool to the backend");
    println!("  4. Messages will be processed with timeout protection\n");

    println!("Key benefits of this setup:");
    println!("  ✓ Per-worker timeout configuration");
    println!("  ✓ Global timeout middleware as safety net");
    println!("  ✓ Prevents RabbitMQ PRECONDITION_FAILED errors");
    println!("  ✓ Graceful nack with requeue on timeout");
    println!("  ✓ No detached tokio::spawn tasks (controlled concurrency)\n");

    println!("Example complete! The pool is ready to process messages.");
    println!("Press Ctrl+C to exit.");

    // Keep the program running
    tokio::signal::ctrl_c().await?;
    println!("\nShutting down...");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fast_worker_timeout_config() {
        let worker = FastWorker {
            id: "test-fast".to_string(),
        };

        assert_eq!(worker.processing_timeout(), Some(Duration::from_secs(5)));
    }

    #[tokio::test]
    async fn test_slow_worker_timeout_config() {
        let worker = SlowWorker {
            id: "test-slow".to_string(),
        };

        assert_eq!(worker.processing_timeout(), Some(Duration::from_secs(25)));
    }

    #[tokio::test]
    async fn test_very_slow_worker_timeout_config() {
        let worker = VerySlowWorker {
            id: "test-very-slow".to_string(),
        };

        assert_eq!(worker.processing_timeout(), Some(Duration::from_secs(120)));
    }

    #[tokio::test]
    async fn test_timeout_worker_config() {
        let worker = TimeoutWorker {
            id: "test-timeout".to_string(),
        };

        assert_eq!(worker.processing_timeout(), Some(Duration::from_secs(2)));
    }
}
