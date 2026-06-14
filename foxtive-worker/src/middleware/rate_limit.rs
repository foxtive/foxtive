use async_trait::async_trait;
use std::sync::Arc;

use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};

use crate::error::WorkerError;
use crate::message::ReceivedMessage;
use crate::middleware::{MessageHandler, Middleware, MiddlewareResult};

/// Middleware that rate limits message processing using a token bucket algorithm.
///
/// This middleware prevents overwhelming downstream systems by limiting the rate
/// at which messages are processed. When the rate limit is exceeded, messages
/// are rejected with a `WorkerError::RateLimited` error.
///
/// # Example
/// ```rust,no_run
/// use foxtive_worker::RateLimitMiddleware;
///
/// // Allow 100 messages per second with burst capacity of 50
/// let middleware = RateLimitMiddleware::new(100, 50);
/// ```
pub struct RateLimitMiddleware {
    limiter: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
    name: String,
}

impl std::fmt::Debug for RateLimitMiddleware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimitMiddleware")
            .field("name", &self.name)
            .finish()
    }
}

impl RateLimitMiddleware {
    /// Create a new rate limiter middleware.
    ///
    /// # Arguments
    /// * `rate` - Sustained rate in messages per second
    /// * `burst` - Maximum burst capacity (max tokens)
    pub fn new(rate: u64, burst: u32) -> Self {
        let quota = Quota::per_second(
            std::num::NonZeroU32::new(rate as u32).unwrap_or(std::num::NonZeroU32::new(1).unwrap()),
        )
        .allow_burst(
            std::num::NonZeroU32::new(burst).unwrap_or(std::num::NonZeroU32::new(1).unwrap()),
        );

        Self {
            limiter: Arc::new(RateLimiter::direct(quota)),
            name: format!("rate-limit-{}rps", rate),
        }
    }

    /// Create a rate limiter with equal rate and burst capacity.
    pub fn with_rate(rate: u64) -> Self {
        Self::new(rate, rate as u32)
    }
}

#[async_trait]
impl Middleware for RateLimitMiddleware {
    fn name(&self) -> &str {
        &self.name
    }

    async fn handle(
        &self,
        message: ReceivedMessage<serde_json::Value>,
        next: Box<dyn MessageHandler>,
    ) -> Result<MiddlewareResult, WorkerError> {
        match self.limiter.check() {
            Ok(_) => next.handle(message).await,
            Err(_) => Err(WorkerError::ProcessingFailed(format!(
                "Rate limit exceeded for middleware '{}'",
                self.name
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{AckHandle, Message, MessageMetadata};
    use std::sync::Arc;
    use tokio::time::{self, Duration};

    struct SuccessHandler;

    #[async_trait]
    impl MessageHandler for SuccessHandler {
        async fn handle(
            &self,
            _message: ReceivedMessage<serde_json::Value>,
        ) -> Result<MiddlewareResult, WorkerError> {
            Ok(MiddlewareResult::Continue)
        }
    }

    fn create_test_message() -> ReceivedMessage<serde_json::Value> {
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
    async fn test_rate_limit_allows_within_limit() {
        let rate = 10;
        let burst = 10;
        let middleware = RateLimitMiddleware::new(rate, burst);

        // Should allow up to burst capacity
        for _ in 0..burst {
            let message = create_test_message();
            assert!(
                middleware
                    .handle(message, Box::new(SuccessHandler))
                    .await
                    .is_ok()
            );
        }
    }

    #[tokio::test]
    async fn test_rate_limit_rejects_over_limit() {
        let rate = 10;
        let burst = 5;
        let middleware = RateLimitMiddleware::new(rate, burst);

        // Consume all tokens
        for _ in 0..burst {
            let message = create_test_message();
            middleware
                .handle(message, Box::new(SuccessHandler))
                .await
                .unwrap();
        }

        // Next should fail
        let message = create_test_message();
        let result = middleware.handle(message, Box::new(SuccessHandler)).await;
        assert!(result.is_err());
        if let Err(WorkerError::ProcessingFailed(_)) = result {
            // Success: rate limited
        } else {
            panic!("Expected ProcessingFailed (rate limited)");
        }
    }

    #[tokio::test]
    async fn test_rate_limit_with_custom_burst() {
        let rate = 100;
        let burst = 20;
        let middleware = RateLimitMiddleware::new(rate, burst);

        // Should allow burst of 20
        for _ in 0..burst {
            let message = create_test_message();
            assert!(
                middleware
                    .handle(message, Box::new(SuccessHandler))
                    .await
                    .is_ok()
            );
        }

        // 21st should fail
        let message = create_test_message();
        assert!(
            middleware
                .handle(message, Box::new(SuccessHandler))
                .await
                .is_err()
        );

        let message = create_test_message();
        if let Err(WorkerError::ProcessingFailed(_)) =
            middleware.handle(message, Box::new(SuccessHandler)).await
        {
            // Success: rate limited
        } else {
            panic!("Expected ProcessingFailed (rate limited)");
        }
    }

    #[tokio::test]
    async fn test_rate_limit_refills_over_time() {
        let rate = 1; // 1 message per second
        let burst = 1;
        let middleware = RateLimitMiddleware::new(rate, burst);

        // Consume the initial token
        let message = create_test_message();
        assert!(
            middleware
                .handle(message, Box::new(SuccessHandler))
                .await
                .is_ok()
        );

        // Next message should be rate-limited immediately
        let message = create_test_message();
        assert!(
            middleware
                .handle(message, Box::new(SuccessHandler))
                .await
                .is_err()
        );

        // Wait for a token to refill (slightly more than 1 second)
        time::sleep(Duration::from_millis(1100)).await;

        // Now it should allow another message
        let message = create_test_message();
        assert!(
            middleware
                .handle(message, Box::new(SuccessHandler))
                .await
                .is_ok()
        );
    }
}
