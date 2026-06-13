use async_trait::async_trait;

use crate::message::ReceivedMessage;
use crate::middleware::{MessageHandler, Middleware, MiddlewareResult};

/// Middleware that adds distributed tracing to message processing.
///
/// This middleware creates trace spans for each message, capturing:
/// - Message ID and metadata
/// - Processing duration
/// - Success/failure status
/// - Error details (if any)
///
/// When the `tracing` feature is enabled, it uses the `tracing` crate
/// to emit structured events. Otherwise, it logs to stdout.
///
/// # Example
/// ```rust,no_run
/// use foxtive_worker::TracingMiddleware;
///
/// let middleware = TracingMiddleware::new("my-service");
/// ```
pub struct TracingMiddleware {
    service_name: String,
}

impl std::fmt::Debug for TracingMiddleware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TracingMiddleware")
            .field("service_name", &self.service_name)
            .finish()
    }
}

impl TracingMiddleware {
    /// Create a new tracing middleware.
    ///
    /// # Arguments
    /// * `service_name` - Name of the service for trace identification
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
        }
    }
}

#[async_trait]
impl Middleware for TracingMiddleware {
    fn name(&self) -> &str {
        "tracing"
    }

    async fn handle(
        &self,
        message: ReceivedMessage<serde_json::Value>,
        next: Box<dyn MessageHandler>,
    ) -> Result<MiddlewareResult, crate::error::WorkerError> {
        let message_id = message.message.id.clone();
        let source = message.message.metadata.source.clone();

        // Log message reception
        #[cfg(feature = "tracing")]
        {
            tracing::info!(
                service = self.service_name.as_str(),
                message_id = message_id.as_str(),
                source = source.as_str(),
                attempt = message.message.metadata.attempt,
                "Processing message"
            );
        }

        #[cfg(not(feature = "tracing"))]
        {
            println!(
                "[{}] Processing message {} from {} (attempt {})",
                self.service_name, message_id, source, message.message.metadata.attempt
            );
        }

        let start_time = std::time::Instant::now();

        // Process the message
        let result = next.handle(message).await;

        let elapsed = start_time.elapsed();

        // Log completion
        match &result {
            Ok(MiddlewareResult::Continue) | Ok(MiddlewareResult::Acknowledged) => {
                #[cfg(feature = "tracing")]
                {
                    tracing::info!(
                        service = self.service_name.as_str(),
                        message_id = message_id.as_str(),
                        duration_ms = elapsed.as_millis(),
                        "Message processed successfully"
                    );
                }

                #[cfg(not(feature = "tracing"))]
                {
                    println!(
                        "[{}] ✓ Message {} processed in {}ms",
                        self.service_name,
                        message_id,
                        elapsed.as_millis()
                    );
                }
            }
            Err(e) => {
                #[cfg(feature = "tracing")]
                {
                    tracing::error!(
                        service = self.service_name.as_str(),
                        message_id = message_id.as_str(),
                        duration_ms = elapsed.as_millis(),
                        error = e.to_string().as_str(),
                        "Message processing failed"
                    );
                }

                #[cfg(not(feature = "tracing"))]
                {
                    println!(
                        "[{}] ✗ Message {} failed after {}ms: {}",
                        self.service_name,
                        message_id,
                        elapsed.as_millis(),
                        e
                    );
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct SuccessHandler;

    #[async_trait]
    impl MessageHandler for SuccessHandler {
        async fn handle(
            &self,
            _message: ReceivedMessage<serde_json::Value>,
        ) -> Result<MiddlewareResult, crate::error::WorkerError> {
            Ok(MiddlewareResult::Continue)
        }
    }

    struct FailureHandler;

    #[async_trait]
    impl MessageHandler for FailureHandler {
        async fn handle(
            &self,
            _message: ReceivedMessage<serde_json::Value>,
        ) -> Result<MiddlewareResult, crate::error::WorkerError> {
            Err(crate::error::WorkerError::ProcessingFailed(
                "test error".to_string(),
            ))
        }
    }

    fn create_test_message() -> ReceivedMessage<serde_json::Value> {
        use crate::message::{AckHandle, Message, MessageMetadata};

        #[derive(Debug)]
        struct MockAckHandle;

        #[async_trait]
        impl AckHandle for MockAckHandle {
            async fn ack(&self) -> crate::WorkerResult<()> {
                Ok(())
            }

            async fn nack(&self, _requeue: bool) -> crate::WorkerResult<()> {
                Ok(())
            }
        }

        let message = Message {
            id: "test-1".to_string(),
            payload: serde_json::json!({"test": "data"}),
            metadata: MessageMetadata::new("test-queue"),
        };
        ReceivedMessage::new(message, Arc::new(MockAckHandle))
    }

    #[tokio::test]
    async fn test_tracing_middleware_success() {
        let middleware = TracingMiddleware::new("test-service");
        let message = create_test_message();

        let result = middleware.handle(message, Box::new(SuccessHandler)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_tracing_middleware_failure() {
        let middleware = TracingMiddleware::new("test-service");
        let message = create_test_message();

        let result = middleware.handle(message, Box::new(FailureHandler)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_tracing_middleware_name() {
        let middleware = TracingMiddleware::new("my-service");
        assert_eq!(middleware.name(), "tracing");
    }
}
