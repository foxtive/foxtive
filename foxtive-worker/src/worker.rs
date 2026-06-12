use async_trait::async_trait;
use std::time::Duration;

use crate::error::WorkerResult;
use crate::message::ReceivedMessage;

/// Backoff strategy for retries and restarts.
#[derive(Debug, Clone)]
pub enum BackoffStrategy {
    /// Fixed delay between attempts.
    Fixed(Duration),

    /// Exponential backoff with optional jitter.
    Exponential {
        /// Initial delay
        initial: Duration,
        /// Maximum delay cap
        max: Duration,
        /// Multiplier for exponential growth (e.g., 2.0 doubles each time)
        multiplier: f64,
    },
}

impl BackoffStrategy {
    /// Calculate the delay for a given attempt number (0-indexed).
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        match self {
            BackoffStrategy::Fixed(duration) => *duration,
            BackoffStrategy::Exponential {
                initial,
                max,
                multiplier,
            } => {
                let delay = initial.mul_f64(multiplier.powi(attempt as i32));
                delay.min(*max)
            }
        }
    }
}

/// Core worker interface that all workers must implement.
///
/// Workers are responsible for processing individual messages from a backend.
/// They can be supervised by `foxtive-supervisor` for automatic restarts on failure.
///
/// # Two-Level Backoff System
///
/// Foxtive Worker uses two independent backoff strategies:
///
/// 1. **Worker Restart Backoff** (this trait): Controls how quickly a crashed/failed
///    worker is restarted by the supervisor. This handles worker-level failures like
///    panics, setup failures, or connection losses.
///
/// 2. **Message Retry Backoff** (RetryHandler middleware): Controls delays between
///    retry attempts for individual failed messages. This is configured separately
///    in the middleware pipeline.
///
/// # Example
/// ```rust
/// use foxtive_worker::{Worker, ReceivedMessage};
/// use foxtive_worker::error::WorkerResult;
/// use async_trait::async_trait;
///
/// struct MyWorker;
///
/// #[async_trait]
/// impl Worker for MyWorker {
///     fn id(&self) -> &str { "my-worker" }
///     
///     async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
///         println!("Processing message: {}", message.message.id);
///         // Your processing logic here
///         message.ack().await?;
///         Ok(())
///     }
/// }
/// ```
#[async_trait]
pub trait Worker: Send + Sync {
    /// Unique worker identifier.
    ///
    /// This should be stable across restarts and unique within a worker pool.
    fn id(&self) -> &str;

    /// Human-readable name for the worker.
    ///
    /// Used for logging and monitoring. Defaults to the worker ID if not overridden.
    fn name(&self) -> String {
        self.id().to_string()
    }

    /// Process a single message.
    ///
    /// This is the core method where message processing logic is implemented.
    /// The worker should acknowledge the message on success or negative-acknowledge
    /// on failure.
    ///
    /// # Arguments
    /// * `message` - The received message with acknowledgment capability
    ///
    /// # Returns
    /// * `Ok(())` if processing succeeded
    /// * `Err(WorkerError)` if processing failed
    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()>;

    /// Optional setup before worker starts.
    ///
    /// This method is called once when the worker is initialized, before any
    /// messages are processed. Use it for one-time initialization like
    /// establishing connections or loading configuration.
    ///
    /// # Returns
    /// * `Ok(())` if setup succeeded
    /// * `Err(WorkerError)` if setup failed (worker will not start)
    async fn setup(&self) -> WorkerResult<()> {
        Ok(())
    }

    /// Optional cleanup on shutdown.
    ///
    /// This method is called when the worker is being shut down gracefully.
    /// Use it to release resources, close connections, or flush buffers.
    async fn teardown(&self) {}

    /// Concurrency limit for this worker.
    ///
    /// If `Some(n)`, the worker will process at most `n` messages concurrently.
    /// If `None`, there is no limit (bounded only by system resources).
    ///
    /// This is useful for preventing resource exhaustion when processing
    /// expensive messages.
    fn concurrency_limit(&self) -> Option<usize> {
        None
    }

    /// Backoff strategy for worker-level restarts.
    ///
    /// This controls how quickly a crashed or failed worker is restarted
    /// by the supervisor. It is **independent** of message-level retry backoff
    /// (which is handled by the RetryHandler middleware).
    ///
    /// # When This Applies
    /// - Worker panics during message processing
    /// - Worker setup fails
    /// - Worker encounters unrecoverable errors
    /// - Supervisor detects worker health check failures
    ///
    /// # When This Does NOT Apply
    /// - Individual message processing failures (handled by RetryHandler)
    /// - Graceful worker shutdown
    ///
    /// # Default
    /// Exponential backoff starting at 1 second, max 60 seconds, multiplier 2.0
    fn restart_backoff_strategy(&self) -> BackoffStrategy {
        BackoffStrategy::Exponential {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(60),
            multiplier: 2.0,
        }
    }

    /// Optional processing timeout for individual messages.
    ///
    /// If `Some(duration)`, each message processed by this worker will have a maximum
    /// processing time enforced. If processing exceeds this timeout, the message will
    /// be negative-acknowledged (nacked) with requeue, preventing RabbitMQ consumer
    /// timeout errors.
    ///
    /// This is especially important when:
    /// - Processing times are variable or unpredictable
    /// - External dependencies (APIs, databases) might be slow
    /// - You want to fail fast rather than wait for broker timeouts
    ///
    /// # Relationship to Broker Timeouts
    /// Set this to be **less than** your broker's consumer timeout to ensure
    /// graceful handling before the broker kills the connection.
    ///
    /// Example: If RabbitMQ has a 30-second consumer timeout, set this to 25 seconds.
    ///
    /// # Default
    /// `None` - No timeout enforcement (relies on broker timeouts)
    ///
    /// # Example
    /// ```rust
    /// use foxtive_worker::Worker;
    /// use std::time::Duration;
    ///
    /// struct SlowWorker;
    ///
    /// #[async_trait::async_trait]
    /// impl Worker for SlowWorker {
    ///     fn id(&self) -> &str { "slow-worker" }
    ///     
    ///     // This worker can take up to 2 minutes per message
    ///     fn processing_timeout(&self) -> Option<Duration> {
    ///         Some(Duration::from_secs(120))
    ///     }
    ///     
    ///     async fn process(&self, message: foxtive_worker::ReceivedMessage<serde_json::Value>) 
    ///         -> foxtive_worker::error::WorkerResult<()> {
    ///         // Long-running processing...
    ///         Ok(())
    ///     }
    /// }
    /// ```
    fn processing_timeout(&self) -> Option<Duration> {
        None
    }
}

/// Helper function to create a boxed worker trait object.
pub fn box_worker<W: Worker + 'static>(worker: W) -> Box<dyn Worker> {
    Box::new(worker)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_backoff() {
        let strategy = BackoffStrategy::Fixed(Duration::from_secs(5));

        assert_eq!(strategy.delay_for_attempt(0), Duration::from_secs(5));
        assert_eq!(strategy.delay_for_attempt(1), Duration::from_secs(5));
        assert_eq!(strategy.delay_for_attempt(10), Duration::from_secs(5));
    }

    #[test]
    fn test_exponential_backoff() {
        let strategy = BackoffStrategy::Exponential {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(60),
            multiplier: 2.0,
        };

        assert_eq!(strategy.delay_for_attempt(0), Duration::from_secs(1));
        assert_eq!(strategy.delay_for_attempt(1), Duration::from_secs(2));
        assert_eq!(strategy.delay_for_attempt(2), Duration::from_secs(4));
        assert_eq!(strategy.delay_for_attempt(3), Duration::from_secs(8));
        assert_eq!(strategy.delay_for_attempt(4), Duration::from_secs(16));
        assert_eq!(strategy.delay_for_attempt(5), Duration::from_secs(32));
        // Should be capped at max
        assert_eq!(strategy.delay_for_attempt(10), Duration::from_secs(60));
    }

    #[test]
    fn test_worker_default_methods() {
        struct TestWorker;

        #[async_trait]
        impl Worker for TestWorker {
            fn id(&self) -> &str {
                "test-worker"
            }

            async fn process(
                &self,
                _message: ReceivedMessage<serde_json::Value>,
            ) -> WorkerResult<()> {
                Ok(())
            }
        }

        let worker = TestWorker;
        assert_eq!(worker.id(), "test-worker");
        assert_eq!(worker.name(), "test-worker");
        assert!(worker.concurrency_limit().is_none());

        let strategy = worker.restart_backoff_strategy();
        match strategy {
            BackoffStrategy::Exponential {
                initial,
                max,
                multiplier,
            } => {
                assert_eq!(initial, Duration::from_secs(1));
                assert_eq!(max, Duration::from_secs(60));
                assert_eq!(multiplier, 2.0);
            }
            _ => panic!("Expected Exponential strategy"),
        }
    }
}
