use async_trait::async_trait;

use crate::error::{WorkerError, WorkerResult};
use crate::message::ReceivedMessage;
use crate::middleware::{MessageHandler, Middleware};

/// Middleware that automatically acknowledges or negative-acknowledges messages
/// based on processing results.
///
/// This middleware wraps message processing and:
/// - Calls `ack()` if processing succeeds (when `ack_on_success` is true)
/// - Calls `nack()` if processing fails (when `nack_on_failure` is true)
///
/// # Example
/// ```rust,no_run
/// use foxtive_worker::AckNackMiddleware;
///
/// // Auto-ack on success, auto-nack with requeue on failure
/// let middleware = AckNackMiddleware::default();
/// ```
#[derive(Debug, Clone)]
pub struct AckNackMiddleware {
    /// Whether to acknowledge messages on successful processing
    pub ack_on_success: bool,
    
    /// Whether to negative-acknowledge messages on failed processing
    pub nack_on_failure: bool,
    
    /// Whether to requeue messages when nacking
    pub requeue_on_nack: bool,
}

impl Default for AckNackMiddleware {
    fn default() -> Self {
        Self {
            ack_on_success: true,
            nack_on_failure: true,
            requeue_on_nack: true,
        }
    }
}

impl AckNackMiddleware {
    /// Create a new AckNackMiddleware with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new AckNackMiddleware with custom settings.
    pub fn with_config(ack_on_success: bool, nack_on_failure: bool, requeue_on_nack: bool) -> Self {
        Self {
            ack_on_success,
            nack_on_failure,
            requeue_on_nack,
        }
    }
}

#[async_trait]
impl Middleware for AckNackMiddleware {
    fn name(&self) -> &str {
        "ack-nack"
    }

    async fn handle(
        &self,
        message: ReceivedMessage<serde_json::Value>,
        next: Box<dyn MessageHandler>,
    ) -> WorkerResult<()> {
        let result = next.handle(message.clone()).await;

        match result {
            Ok(()) if self.ack_on_success => {
                // Acknowledge successful processing
                message.ack().await.map_err(|e| {
                    tracing::error!("Failed to ack message {}: {}", message.message.id, e);
                    WorkerError::AcknowledgmentFailed(format!(
                        "Message {} processed successfully but ack failed: {}",
                        message.message.id, e
                    ))
                })?;
                Ok(())
            }
            Err(e) if self.nack_on_failure => {
                // Negative-acknowledge failed processing
                if let Err(nack_err) = message.nack(self.requeue_on_nack).await {
                    tracing::error!(
                        "Failed to nack message {}: {} (original error: {})",
                        message.message.id,
                        nack_err,
                        e
                    );
                    // Return combined error to inform upstream about both failures
                    return Err(WorkerError::AcknowledgmentFailed(format!(
                        "Message {} processing failed and nack also failed: {} (original: {})",
                        message.message.id, nack_err, e
                    )));
                }
                // Successfully nacked - return special error to prevent pool from nacking again
                Err(WorkerError::AlreadyAcknowledged)
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Message, MessageMetadata, AckHandle};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

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
        async fn ack(&self) -> WorkerResult<()> {
            self.acked.store(true, Ordering::SeqCst);
            Ok(())
        }

        async fn nack(&self, requeue: bool) -> WorkerResult<()> {
            self.nacked.store(true, Ordering::SeqCst);
            self.requeued.store(requeue, Ordering::SeqCst);
            Ok(())
        }
    }

    struct SuccessHandler;

    #[async_trait]
    impl MessageHandler for SuccessHandler {
        async fn handle(&self, _message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
            Ok(())
        }
    }

    struct FailureHandler;

    #[async_trait]
    impl MessageHandler for FailureHandler {
        async fn handle(&self, _message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
            Err(crate::error::WorkerError::ProcessingFailed("test error".to_string()))
        }
    }

    fn create_test_message() -> (ReceivedMessage<serde_json::Value>, Arc<AtomicBool>, Arc<AtomicBool>, Arc<AtomicBool>) {
        let (ack_handle, acked, nacked, requeued) = MockAckHandle::new();
        let message = Message {
            id: "test-1".to_string(),
            payload: serde_json::json!({"test": "data"}),
            metadata: MessageMetadata::new("test-queue"),
        };
        (ReceivedMessage::new(message, Arc::new(ack_handle)), acked, nacked, requeued)
    }

    #[tokio::test]
    async fn test_ack_on_success() {
        let middleware = AckNackMiddleware::new();
        let (message, acked, nacked, _) = create_test_message();

        middleware.handle(message, Box::new(SuccessHandler)).await.unwrap();

        assert!(acked.load(Ordering::SeqCst));
        assert!(!nacked.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_nack_on_failure() {
        let middleware = AckNackMiddleware::new();
        let (message, acked, nacked, requeued) = create_test_message();

        let result = middleware.handle(message, Box::new(FailureHandler)).await;
        assert!(result.is_err());

        assert!(!acked.load(Ordering::SeqCst));
        assert!(nacked.load(Ordering::SeqCst));
        assert!(requeued.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_no_ack_on_success_when_disabled() {
        let middleware = AckNackMiddleware::with_config(false, true, true);
        let (message, acked, _, _) = create_test_message();

        middleware.handle(message, Box::new(SuccessHandler)).await.unwrap();

        assert!(!acked.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_no_nack_on_failure_when_disabled() {
        let middleware = AckNackMiddleware::with_config(true, false, true);
        let (message, _, nacked, _) = create_test_message();

        let _ = middleware.handle(message, Box::new(FailureHandler)).await;

        assert!(!nacked.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_nack_without_requeue() {
        let middleware = AckNackMiddleware::with_config(true, true, false);
        let (message, _, nacked, requeued) = create_test_message();

        let _ = middleware.handle(message, Box::new(FailureHandler)).await;

        assert!(nacked.load(Ordering::SeqCst));
        assert!(!requeued.load(Ordering::SeqCst));
    }
}
