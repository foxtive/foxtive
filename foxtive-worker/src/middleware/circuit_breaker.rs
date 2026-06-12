use async_trait::async_trait;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::error::{WorkerError, WorkerResult};
use crate::message::ReceivedMessage;
use crate::middleware::{MessageHandler, Middleware};

/// Circuit breaker state.
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState {
    /// Circuit is closed, requests flow normally
    Closed,

    /// Circuit is open, requests are rejected
    Open,

    /// Circuit is half-open, testing if service recovered
    HalfOpen,
}

/// Circuit breaker configuration and state.
struct CircuitBreakerState {
    /// Current state of the circuit breaker
    state: CircuitState,
    /// Number of consecutive failures
    failure_count: u32,
    /// Maximum failures before opening circuit
    max_failures: u32,
    /// Time to wait before transitioning from Open to HalfOpen
    timeout: Duration,
    /// When the circuit was opened
    opened_at: Option<Instant>,
    /// Number of successes in HalfOpen state needed to close
    success_threshold: u32,
    /// Current successes in HalfOpen state
    half_open_successes: u32,
    /// Flag to ensure only one request is allowed through in HalfOpen state
    test_request_in_progress: bool,
}

impl CircuitBreakerState {
    fn new(max_failures: u32, timeout: Duration, success_threshold: u32) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            max_failures,
            timeout,
            opened_at: None,
            success_threshold,
            half_open_successes: 0,
            test_request_in_progress: false, // Initialize the new flag
        }
    }

    fn should_allow_request(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has elapsed
                if let Some(opened_at) = self.opened_at
                    && opened_at.elapsed() >= self.timeout
                {
                    // Transition to HalfOpen and allow one test request
                    self.state = CircuitState::HalfOpen;
                    self.half_open_successes = 0;
                    self.test_request_in_progress = true; // This request is the test request
                    return true;
                }
                false // Still in Open state, timeout not elapsed
            }
            CircuitState::HalfOpen => {
                // Only allow one request through in HalfOpen state
                if !self.test_request_in_progress {
                    self.test_request_in_progress = true;
                    true
                } else {
                    false // Another request is already testing
                }
            }
        }
    }

    fn record_success(&mut self) {
        match self.state {
            CircuitState::Closed => {
                // Reset failure count on success
                self.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                // Don't reset test_request_in_progress here - keep blocking additional requests
                self.half_open_successes += 1;
                if self.half_open_successes >= self.success_threshold {
                    // Transition to Closed
                    self.state = CircuitState::Closed;
                    self.failure_count = 0;
                    self.opened_at = None;
                    self.test_request_in_progress = false; // Reset flag when closing circuit
                }
            }
            CircuitState::Open => {
                // Should not happen if `should_allow_request` is respected
            }
        }
    }

    fn record_failure(&mut self) {
        match self.state {
            CircuitState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= self.max_failures {
                    // Open the circuit
                    self.state = CircuitState::Open;
                    self.opened_at = Some(Instant::now());
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in HalfOpen reopens the circuit
                self.test_request_in_progress = false; // Test request completed (with failure)
                self.state = CircuitState::Open;
                self.opened_at = Some(Instant::now());
                self.half_open_successes = 0;
            }
            CircuitState::Open => {
                // Already open, do nothing
            }
        }
    }

    fn current_state(&self) -> &CircuitState {
        &self.state
    }
}

/// Middleware that implements the circuit breaker pattern.
///
/// The circuit breaker protects downstream services from being overwhelmed
/// when they're failing. It has three states:
/// - **Closed**: Normal operation, requests pass through
/// - **Open**: Requests are rejected immediately (fail fast)
/// - **HalfOpen**: Testing if service recovered with limited requests
///
/// # Example
/// ```rust,no_run
/// use foxtive_worker::CircuitBreakerMiddleware;
/// use std::time::Duration;
///
/// // Open circuit after 5 failures, retry after 30 seconds
/// let middleware = CircuitBreakerMiddleware::new(5, Duration::from_secs(30));
/// ```
pub struct CircuitBreakerMiddleware {
    state: Arc<Mutex<CircuitBreakerState>>,
    name: String,
}

impl std::fmt::Debug for CircuitBreakerMiddleware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircuitBreakerMiddleware")
            .field("name", &self.name)
            .finish()
    }
}

impl CircuitBreakerMiddleware {
    /// Create a new circuit breaker middleware.
    ///
    /// # Arguments
    /// * `max_failures` - Number of consecutive failures before opening circuit
    /// * `timeout` - Time to wait before transitioning from Open to HalfOpen
    pub fn new(max_failures: u32, timeout: Duration) -> Self {
        Self {
            state: Arc::new(Mutex::new(CircuitBreakerState::new(
                max_failures,
                timeout,
                1, // Default: 1 success to close
            ))),
            name: format!("circuit-breaker-{}failures", max_failures),
        }
    }

    /// Create a circuit breaker with custom success threshold.
    pub fn with_threshold(max_failures: u32, timeout: Duration, success_threshold: u32) -> Self {
        Self {
            state: Arc::new(Mutex::new(CircuitBreakerState::new(
                max_failures,
                timeout,
                success_threshold,
            ))),
            name: format!("circuit-breaker-{}failures", max_failures),
        }
    }

    /// Get the current circuit state.
    pub async fn get_state(&self) -> CircuitState {
        let mut state = self.state.lock().await;
        // Check if Open state should transition to HalfOpen
        if state.current_state() == &CircuitState::Open
            && let Some(opened_at) = state.opened_at
            && opened_at.elapsed() >= state.timeout
        {
            state.state = CircuitState::HalfOpen;
            state.half_open_successes = 0;
            // Don't reset test_request_in_progress here - it will be set by should_allow_request
        }
        state.current_state().clone()
    }
}

#[async_trait]
impl Middleware for CircuitBreakerMiddleware {
    fn name(&self) -> &str {
        &self.name
    }

    async fn handle(
        &self,
        message: ReceivedMessage<serde_json::Value>,
        next: Box<dyn MessageHandler>,
    ) -> WorkerResult<()> {
        // Check if request should be allowed
        {
            let mut state = self.state.lock().await;
            if !state.should_allow_request() {
                return Err(WorkerError::ProcessingFailed(format!(
                    "Circuit breaker '{}' is open, rejecting request",
                    self.name
                )));
            }
        }

        // Process the message
        let result = next.handle(message).await;

        // Record success or failure
        {
            let mut state = self.state.lock().await;
            match result {
                Ok(_) => state.record_success(),
                Err(_) => state.record_failure(),
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time;

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
            Err(WorkerError::ProcessingFailed("test failure".to_string()))
        }
    }

    fn create_test_message() -> ReceivedMessage<serde_json::Value> {
        use crate::message::{AckHandle, Message, MessageMetadata};

        #[derive(Debug)]
        struct MockAckHandle;

        #[async_trait]
        impl AckHandle for MockAckHandle {
            async fn ack(&self) -> WorkerResult<()> {
                Ok(())
            }

            async fn nack(&self, _requeue: bool) -> WorkerResult<()> {
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
    async fn test_circuit_closed_initially() {
        let middleware = CircuitBreakerMiddleware::new(3, Duration::from_secs(1));
        assert_eq!(middleware.get_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_opens_after_max_failures() {
        let middleware = CircuitBreakerMiddleware::new(3, Duration::from_secs(1));

        // Cause 3 failures
        for _ in 0..3 {
            let message = create_test_message();
            let _ = middleware.handle(message, Box::new(FailureHandler)).await;
        }

        assert_eq!(middleware.get_state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_rejects_when_open() {
        let middleware = CircuitBreakerMiddleware::new(2, Duration::from_secs(1));

        // Open the circuit
        for _ in 0..2 {
            let message = create_test_message();
            let _ = middleware.handle(message, Box::new(FailureHandler)).await;
        }

        // Next request should be rejected
        let message = create_test_message();
        let result = middleware.handle(message, Box::new(SuccessHandler)).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(WorkerError::ProcessingFailed(_))));
    }

    #[tokio::test]
    async fn test_circuit_transitions_to_half_open_and_allows_one_request() {
        let middleware = CircuitBreakerMiddleware::new(2, Duration::from_millis(100));

        // Open the circuit
        for _ in 0..2 {
            let message = create_test_message();
            let _ = middleware.handle(message, Box::new(FailureHandler)).await;
        }

        assert_eq!(middleware.get_state().await, CircuitState::Open);

        // Wait for timeout
        time::sleep(Duration::from_millis(150)).await;

        // Should transition to HalfOpen and allow the first request
        let message1 = create_test_message();
        let result1 = middleware.handle(message1, Box::new(SuccessHandler)).await;
        assert!(result1.is_ok());
        assert_eq!(middleware.get_state().await, CircuitState::Closed); // Should close after 1 success with default threshold

        // If it were still HalfOpen, a second request should be rejected
        let middleware_half_open_test =
            CircuitBreakerMiddleware::with_threshold(2, Duration::from_millis(100), 2); // Need 2 successes to close
        for _ in 0..2 {
            let message = create_test_message();
            let _ = middleware_half_open_test
                .handle(message, Box::new(FailureHandler))
                .await;
        }
        time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            middleware_half_open_test.get_state().await,
            CircuitState::HalfOpen
        );

        let message_test_1 = create_test_message();
        assert!(
            middleware_half_open_test
                .handle(message_test_1, Box::new(SuccessHandler))
                .await
                .is_ok()
        );
        assert_eq!(
            middleware_half_open_test.get_state().await,
            CircuitState::HalfOpen
        ); // Still HalfOpen, 1 success recorded

        let message_test_2 = create_test_message();
        let result_test_2 = middleware_half_open_test
            .handle(message_test_2, Box::new(SuccessHandler))
            .await;
        assert!(result_test_2.is_err()); // Second request should be rejected
        assert!(matches!(
            result_test_2,
            Err(WorkerError::ProcessingFailed(_))
        ));
        assert_eq!(
            middleware_half_open_test.get_state().await,
            CircuitState::HalfOpen
        ); // Still HalfOpen
    }

    #[tokio::test]
    async fn test_circuit_closes_after_success_in_half_open() {
        let middleware = CircuitBreakerMiddleware::new(2, Duration::from_millis(100));

        // Open the circuit
        for _ in 0..2 {
            let message = create_test_message();
            let _ = middleware.handle(message, Box::new(FailureHandler)).await;
        }

        // Wait for timeout
        time::sleep(Duration::from_millis(150)).await;

        // Success in HalfOpen should close circuit (with default success_threshold = 1)
        let message = create_test_message();
        middleware
            .handle(message, Box::new(SuccessHandler))
            .await
            .unwrap();

        assert_eq!(middleware.get_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_reopens_on_failure_in_half_open() {
        let middleware = CircuitBreakerMiddleware::new(2, Duration::from_millis(100));

        // Open the circuit
        for _ in 0..2 {
            let message = create_test_message();
            let _ = middleware.handle(message, Box::new(FailureHandler)).await;
        }

        // Wait for timeout
        time::sleep(Duration::from_millis(150)).await;

        // Failure in HalfOpen should reopen circuit
        let message = create_test_message();
        let _ = middleware.handle(message, Box::new(FailureHandler)).await;

        assert_eq!(middleware.get_state().await, CircuitState::Open);
    }
}
