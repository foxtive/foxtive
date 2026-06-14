use async_trait::async_trait;
use std::time::Duration;

use crate::error::WorkerError;
use crate::message::ReceivedMessage;
use crate::middleware::{MessageHandler, Middleware, MiddlewareResult};

/// Middleware that enforces a processing timeout on message handling.
///
/// This middleware wraps message processing with `tokio::time::timeout` to ensure
/// that messages don't exceed a maximum processing time. If processing exceeds the
/// timeout, the message is negative-acknowledged (nacked) with requeue before the
/// broker's consumer timeout can kill the connection.
///
/// # Why This Matters
///
/// Message brokers like RabbitMQ have consumer timeouts (default 30 seconds).
/// If a worker receives a message but doesn't ack/nack within that window,
/// the broker assumes the consumer is dead and closes the channel with a
/// PRECONDITION_FAILED error.
///
/// This middleware prevents that by enforcing a timeout **shorter than** the
/// broker's timeout, ensuring graceful nack with proper error handling.
///
/// # Example
/// ```rust,no_run
/// use foxtive_worker::ProcessingTimeoutMiddleware;
/// use std::time::Duration;
///
/// // Enforce 25-second timeout (less than RabbitMQ's 30s default)
/// let middleware = ProcessingTimeoutMiddleware::new(Duration::from_secs(25));
/// ```
///
/// # Architecture
///
/// The middleware uses `tokio::time::timeout` which does **NOT** spawn detached tasks.
/// It runs the future inline and cancels it if the timeout expires. This maintains
/// controlled concurrency without unbounded task spawning.
///
/// ```text
/// Message → [Timeout Check] → [Next Handler] → Result
///              ↓ (if timeout)
///           Nack + Error
/// ```
#[derive(Debug, Clone)]
pub struct ProcessingTimeoutMiddleware {
    /// Maximum allowed processing time per message
    timeout: Duration,
}

impl ProcessingTimeoutMiddleware {
    /// Create a new processing timeout middleware.
    ///
    /// # Arguments
    /// * `timeout` - Maximum time allowed for message processing
    ///
    /// # Panics
    /// Panics if timeout is zero
    pub fn new(timeout: Duration) -> Self {
        assert!(
            !timeout.is_zero(),
            "Processing timeout must be greater than zero"
        );
        Self { timeout }
    }

    /// Get the configured timeout duration.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

#[async_trait]
impl Middleware for ProcessingTimeoutMiddleware {
    fn name(&self) -> &str {
        "processing-timeout"
    }

    async fn handle(
        &self,
        message: ReceivedMessage<serde_json::Value>,
        next: Box<dyn MessageHandler>,
    ) -> Result<MiddlewareResult, WorkerError> {
        let message_id = message.message.id.clone();

        tracing::debug!(
            message_id = %message_id,
            timeout_ms = self.timeout.as_millis(),
            "Starting message processing with timeout"
        );

        // Use tokio::time::timeout to enforce the limit
        // This does NOT spawn a detached task - it polls the future inline
        // and cancels it if the timeout expires
        match tokio::time::timeout(self.timeout, next.handle(message.clone())).await {
            Ok(result) => {
                // Processing completed within timeout
                result
            }
            Err(_) => {
                // Timeout expired! Nack the message before broker kills us
                tracing::warn!(
                    message_id = %message_id,
                    timeout_ms = self.timeout.as_millis(),
                    "Message processing timed out - nacking with requeue"
                );

                // Attempt to nack with requeue so the message can be retried
                if let Err(nack_err) = message.nack(true).await {
                    tracing::error!(
                        message_id = %message_id,
                        error = %nack_err,
                        "Failed to nack timed-out message"
                    );
                }

                // Return a timeout error
                Err(WorkerError::Timeout(format!(
                    "Message {} processing exceeded timeout of {:?}",
                    message_id, self.timeout
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{AckHandle, Message, MessageMetadata};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Debug)]
    struct MockAckHandle {
        acked: Arc<AtomicBool>,
        nacked: Arc<AtomicBool>,
        requeued: Arc<AtomicBool>,
    }

    impl MockAckHandle {
        fn new() -> (Self, Arc<AtomicBool>, Arc<AtomicBool>, Arc<AtomicBool>) {
            let acked = Arc::new(AtomicBool::new(false));
            let nacked = Arc::new(AtomicBool::new(false));
            let requeued = Arc::new(AtomicBool::new(false));
            (
                Self {
                    acked: acked.clone(),
                    nacked: nacked.clone(),
                    requeued: requeued.clone(),
                },
                acked,
                nacked,
                requeued,
            )
        }
    }

    #[async_trait]
    impl AckHandle for MockAckHandle {
        async fn ack(&self) -> crate::WorkerResult<()> {
            self.acked.store(true, Ordering::SeqCst);
            Ok(())
        }

        async fn nack(&self, requeue: bool) -> crate::WorkerResult<()> {
            self.nacked.store(true, Ordering::SeqCst);
            self.requeued.store(requeue, Ordering::SeqCst);
            Ok(())
        }
    }

    struct FastHandler;

    #[async_trait]
    impl MessageHandler for FastHandler {
        async fn handle(
            &self,
            _message: ReceivedMessage<serde_json::Value>,
        ) -> Result<MiddlewareResult, WorkerError> {
            // Completes immediately
            Ok(MiddlewareResult::Continue)
        }
    }

    struct SlowHandler {
        delay: Duration,
    }

    #[async_trait]
    impl MessageHandler for SlowHandler {
        async fn handle(
            &self,
            _message: ReceivedMessage<serde_json::Value>,
        ) -> Result<MiddlewareResult, WorkerError> {
            tokio::time::sleep(self.delay).await;
            Ok(MiddlewareResult::Continue)
        }
    }

    struct FailingHandler;

    #[async_trait]
    impl MessageHandler for FailingHandler {
        async fn handle(
            &self,
            _message: ReceivedMessage<serde_json::Value>,
        ) -> Result<MiddlewareResult, WorkerError> {
            Err(WorkerError::ProcessingFailed(
                "intentional failure".to_string(),
            ))
        }
    }

    fn create_test_message() -> (
        ReceivedMessage<serde_json::Value>,
        Arc<AtomicBool>,
        Arc<AtomicBool>,
        Arc<AtomicBool>,
    ) {
        let (ack_handle, acked, nacked, requeued) = MockAckHandle::new();
        let message = Message {
            id: "test-msg-1".to_string(),
            payload: serde_json::json!({"test": "data"}),
            metadata: MessageMetadata::new("test-queue"),
        };
        (
            ReceivedMessage::new(message, Arc::new(ack_handle)),
            acked,
            nacked,
            requeued,
        )
    }

    #[tokio::test]
    async fn test_fast_processing_completes() {
        let middleware = ProcessingTimeoutMiddleware::new(Duration::from_secs(5));
        let (message, acked, nacked, _) = create_test_message();

        let result = middleware.handle(message, Box::new(FastHandler)).await;

        assert!(result.is_ok());
        assert!(!acked.load(Ordering::SeqCst)); // Middleware doesn't auto-ack
        assert!(!nacked.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_slow_processing_times_out() {
        let timeout = Duration::from_millis(100);
        let middleware = ProcessingTimeoutMiddleware::new(timeout);
        let (message, _, nacked, requeued) = create_test_message();

        // Handler takes longer than timeout
        let slow_handler = SlowHandler {
            delay: Duration::from_secs(1),
        };

        let result = middleware.handle(message, Box::new(slow_handler)).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), WorkerError::Timeout(_)));
        assert!(nacked.load(Ordering::SeqCst)); // Should nack on timeout
        assert!(requeued.load(Ordering::SeqCst)); // Should requeue
    }

    #[tokio::test]
    async fn test_processing_error_propagates() {
        let middleware = ProcessingTimeoutMiddleware::new(Duration::from_secs(5));
        let (message, _, _, _) = create_test_message();

        let result = middleware.handle(message, Box::new(FailingHandler)).await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WorkerError::ProcessingFailed(_)
        ));
    }

    #[tokio::test]
    async fn test_timeout_cancels_long_running_task() {
        let timeout = Duration::from_millis(50);
        let middleware = ProcessingTimeoutMiddleware::new(timeout);
        let (message, _, nacked, _) = create_test_message();

        // Very slow handler
        let very_slow_handler = SlowHandler {
            delay: Duration::from_secs(10),
        };

        let start = std::time::Instant::now();
        let result = middleware
            .handle(message, Box::new(very_slow_handler))
            .await;
        let elapsed = start.elapsed();

        // Should timeout quickly, not wait for full 10 seconds
        assert!(result.is_err());
        assert!(elapsed < Duration::from_secs(1)); // Should complete in ~50ms, give some buffer
        assert!(nacked.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_boundary_condition_exactly_at_timeout() {
        let timeout = Duration::from_millis(100);
        let middleware = ProcessingTimeoutMiddleware::new(timeout);
        let (message, _, _, _) = create_test_message();

        // Handler completes just before timeout
        let almost_timeout_handler = SlowHandler {
            delay: Duration::from_millis(80),
        };

        let result = middleware
            .handle(message, Box::new(almost_timeout_handler))
            .await;

        // Should succeed (completed before timeout)
        assert!(result.is_ok());
    }

    #[test]
    #[should_panic(expected = "Processing timeout must be greater than zero")]
    fn test_zero_timeout_panics() {
        let _ = ProcessingTimeoutMiddleware::new(Duration::ZERO);
    }

    #[test]
    fn test_timeout_getter() {
        let timeout = Duration::from_secs(30);
        let middleware = ProcessingTimeoutMiddleware::new(timeout);
        assert_eq!(middleware.timeout(), timeout);
    }
}
