use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::backends::{DeadLetterQueueBackend, create_dlq_message};
use crate::dlq::PoisonPillTracker;
use crate::error::WorkerError;
use crate::message::ReceivedMessage;
use crate::middleware::{MessageHandler, Middleware, MiddlewareResult};

/// Configuration for the RetryHandler middleware.
#[derive(Clone)]
pub struct RetryHandlerConfig {
    /// The maximum number of times a message should be retried.
    pub max_retries: u32,
    /// The initial backoff duration for the first retry.
    pub initial_backoff: Duration,
    /// The maximum backoff duration.
    pub max_backoff: Duration,
    /// Multiplier for exponential backoff.
    pub backoff_multiplier: f64,
    /// Optional dead letter queue backend for permanently failed messages.
    pub dead_letter_queue: Option<Arc<DeadLetterQueueBackend>>,
    /// Optional poison pill tracker for detecting problematic messages.
    pub poison_pill_tracker: Option<Arc<PoisonPillTracker>>,
    /// Whether to add jitter to backoff delays (recommended for distributed systems).
    pub use_jitter: bool,
}

impl std::fmt::Debug for RetryHandlerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetryHandlerConfig")
            .field("max_retries", &self.max_retries)
            .field("initial_backoff", &self.initial_backoff)
            .field("max_backoff", &self.max_backoff)
            .field("backoff_multiplier", &self.backoff_multiplier)
            .field(
                "dead_letter_queue",
                &self.dead_letter_queue.as_ref().map(|_| "<MessageBackend>"),
            )
            .field("use_jitter", &self.use_jitter)
            .finish()
    }
}

impl Default for RetryHandlerConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            dead_letter_queue: None,
            poison_pill_tracker: None,
            use_jitter: true,
        }
    }
}

impl RetryHandlerConfig {
    /// Create a new config with custom max retries.
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set the dead letter queue for permanently failed messages.
    pub fn with_dead_letter_queue(mut self, dlq: Arc<DeadLetterQueueBackend>) -> Self {
        self.dead_letter_queue = Some(dlq);
        self
    }

    /// Set the poison pill tracker for detecting problematic messages.
    pub fn with_poison_pill_tracker(mut self, tracker: Arc<PoisonPillTracker>) -> Self {
        self.poison_pill_tracker = Some(tracker);
        self
    }

    /// Enable or disable jitter (enabled by default).
    pub fn with_jitter(mut self, use_jitter: bool) -> Self {
        self.use_jitter = use_jitter;
        self
    }
}

/// Middleware that handles automatic retries for failed messages with exponential backoff.
pub struct RetryHandler {
    config: RetryHandlerConfig,
}

impl RetryHandler {
    /// Creates a new `RetryHandler` with the given configuration.
    pub fn new(config: RetryHandlerConfig) -> Self {
        Self { config }
    }

    /// Calculates the next backoff duration based on the current attempt count.
    /// Uses exponential backoff with optional jitter.
    fn calculate_backoff(&self, attempts: u32) -> Duration {
        if attempts == 0 {
            return self.config.initial_backoff;
        }

        let current_backoff = self.config.initial_backoff.as_secs_f64()
            * self.config.backoff_multiplier.powf(attempts as f64 - 1.0);

        let mut backoff = Duration::from_secs_f64(current_backoff);

        // Add jitter (±25% random variation) to prevent thundering herd
        if self.config.use_jitter {
            // Generate a random factor between -0.25 and +0.25
            let jitter_factor = rand::random::<f64>() * 0.5 - 0.25; // Range: -0.25 to +0.25
            let jitter = backoff.as_secs_f64() * jitter_factor;
            let new_backoff = backoff.as_secs_f64() + jitter;

            // Ensure we don't go below a minimum of 10ms
            backoff = Duration::from_secs_f64(new_backoff.max(0.01));
        }

        std::cmp::min(backoff, self.config.max_backoff)
    }

    /// Send a message to the dead letter queue if configured.
    async fn send_to_dlq(&self, message: &ReceivedMessage<serde_json::Value>, error: &WorkerError) {
        if let Some(ref dlq) = self.config.dead_letter_queue {
            // Check for poison pill first
            let is_poison_pill = if let Some(ref tracker) = self.config.poison_pill_tracker {
                tracker.record_failure(&message.message.id)
            } else {
                false
            };

            if is_poison_pill {
                error!(
                    "[{}] POISON PILL DETECTED: Message {} failed {} times - sending to DLQ",
                    self.name(),
                    message.message.id,
                    message.message.metadata.attempt
                );
            }

            // Create DLQ message with failure context
            let mut dlq_message = create_dlq_message(
                message.message.id.clone(),
                message.message.payload.clone(),
                message.message.metadata.source.clone(),
                message.message.metadata.attempt,
                error,
                None, // Worker ID not available in middleware context
            );

            // Add poison pill flag if detected
            if is_poison_pill {
                dlq_message = dlq_message.with_context("poison_pill", serde_json::json!(true));
            }

            // Send to DLQ backend
            match dlq.send_to_dlq(&dlq_message).await {
                Ok(_) => {
                    info!(
                        "[{}] Successfully sent message {} to DLQ after {} attempts",
                        self.name(),
                        message.message.id,
                        message.message.metadata.attempt
                    );
                }
                Err(e) => {
                    error!(
                        "[{}] Failed to send message {} to DLQ: {:?}",
                        self.name(),
                        message.message.id,
                        e
                    );
                }
            }
        }
    }
}

#[async_trait]
impl Middleware for RetryHandler {
    fn name(&self) -> &str {
        "RetryHandler"
    }

    async fn handle(
        &self,
        mut message: ReceivedMessage<serde_json::Value>,
        next: Box<dyn MessageHandler>,
    ) -> Result<MiddlewareResult, WorkerError> {
        // Increment attempt count before processing
        message.message.metadata.increment_attempt();
        let current_attempts = message.message.metadata.attempt;

        debug!(
            "[{}] Processing message {} (attempt {}/{})",
            self.name(),
            message.message.id,
            current_attempts,
            self.config.max_retries
        );

        let result = next.handle(message.clone()).await;

        match result {
            Ok(middleware_result) => {
                debug!(
                    "[{}] Message {} processed successfully.",
                    self.name(),
                    message.message.id
                );
                Ok(middleware_result)
            }
            Err(e) => {
                warn!(
                    "[{}] Message {} failed on attempt {}: {:?}",
                    self.name(),
                    message.message.id,
                    current_attempts,
                    e
                );

                if current_attempts < self.config.max_retries {
                    let delay = self.calculate_backoff(current_attempts);
                    debug!(
                        "[{}] Message {} will be retried in {:?}. Current attempts: {}",
                        self.name(),
                        message.message.id,
                        delay,
                        current_attempts
                    );
                    Err(WorkerError::RetryableFailure {
                        source: Box::new(e),
                        delay_ms: delay,
                    })
                } else {
                    // Retries exhausted - send to DLQ if configured
                    self.send_to_dlq(&message, &e).await;

                    warn!(
                        "[{}] Retries exhausted for message {} after {} attempts.",
                        self.name(),
                        message.message.id,
                        current_attempts
                    );
                    Err(WorkerError::RetriesExhausted {
                        source: Box::new(e),
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{AckHandle, Message, MessageMetadata, ReceivedMessage};
    use std::sync::Arc;

    #[derive(Debug)]
    struct MockAckHandle;

    #[async_trait::async_trait]
    impl AckHandle for MockAckHandle {
        async fn ack(&self) -> crate::WorkerResult<()> {
            Ok(())
        }
        async fn nack(&self, _requeue: bool) -> crate::WorkerResult<()> {
            Ok(())
        }
    }

    struct FailingHandler {
        fail_count: std::sync::atomic::AtomicUsize,
        fail_until: usize,
    }

    #[async_trait::async_trait]
    impl MessageHandler for FailingHandler {
        async fn handle(&self, _message: ReceivedMessage<serde_json::Value>) -> Result<MiddlewareResult, WorkerError> {
            let count = self
                .fail_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if count < self.fail_until {
                Err(WorkerError::ProcessingError("Simulated failure".into()))
            } else {
                Ok(MiddlewareResult::Continue)
            }
        }
    }

    #[tokio::test]
    async fn test_retry_success_after_failures() {
        let config = RetryHandlerConfig::default().with_max_retries(3);
        let handler = RetryHandler::new(config);

        let inner_handler = FailingHandler {
            fail_count: std::sync::atomic::AtomicUsize::new(0),
            fail_until: 2, // Fail twice, succeed on third
        };

        let message = ReceivedMessage::new(
            Message {
                id: "test-id".to_string(),
                payload: serde_json::json!({}),
                metadata: MessageMetadata::new("test"),
            },
            Arc::new(MockAckHandle),
        );

        let result = handler.handle(message, Box::new(inner_handler)).await;

        // The handler should eventually succeed after retries are handled by the pool/dispatcher
        // In this unit test context, we expect the first call to return a RetryableFailure
        assert!(result.is_err());
        if let Err(WorkerError::RetryableFailure { .. }) = result {
            // Success: it correctly identified a retryable failure
        } else {
            panic!("Expected RetryableFailure");
        }
    }

    #[tokio::test]
    async fn test_retries_exhausted() {
        let config = RetryHandlerConfig::default().with_max_retries(1);
        let handler = RetryHandler::new(config);

        let inner_handler = FailingHandler {
            fail_count: std::sync::atomic::AtomicUsize::new(0),
            fail_until: 10, // Always fail
        };

        let mut message = ReceivedMessage::new(
            Message {
                id: "test-id".to_string(),
                payload: serde_json::json!({}),
                metadata: MessageMetadata::new("test"),
            },
            Arc::new(MockAckHandle),
        );

        // Manually increment attempt to simulate max retries reached
        message.message.metadata.attempt = 1;

        let result = handler.handle(message, Box::new(inner_handler)).await;

        if let Err(WorkerError::RetriesExhausted { .. }) = result {
            // Success: it correctly identified that retries are exhausted
        } else {
            panic!("Expected RetriesExhausted, got: {:?}", result);
        }
    }
}
