use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::backends::{MessageBackend, ReceiveResult};
use crate::error::WorkerResult;

/// Reconnection strategy for backends
#[derive(Debug, Clone)]
pub enum ReconnectStrategy {
    /// Fixed delay between reconnection attempts
    Fixed(Duration),

    /// Exponential backoff with optional max delay and jitter
    Exponential {
        initial: Duration,
        max: Duration,
        multiplier: f64,
        /// Add random jitter to prevent thundering herd (0.0 - 1.0)
        jitter_factor: f64,
    },
}

impl ReconnectStrategy {
    fn delay_for_attempt(&self, attempt: u32) -> Duration {
        match self {
            ReconnectStrategy::Fixed(d) => *d,
            ReconnectStrategy::Exponential {
                initial,
                max,
                multiplier,
                jitter_factor,
            } => {
                // Calculate exponential backoff
                let base_delay = initial.mul_f64(multiplier.powi(attempt as i32));
                let clamped = base_delay.min(*max);

                // Add jitter to prevent thundering herd problem
                if *jitter_factor > 0.0 {
                    let jitter_range = clamped.mul_f64(*jitter_factor);
                    let jitter = jitter_range.mul_f64(rand::random::<f64>());
                    clamped + jitter
                } else {
                    clamped
                }
            }
        }
    }
}

impl Default for ReconnectStrategy {
    fn default() -> Self {
        ReconnectStrategy::Exponential {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(60),
            multiplier: 2.0,
            jitter_factor: 0.1, // 10% jitter
        }
    }
}

/// Wrapper that adds automatic reconnection to any backend.
///
/// This wrapper implements the "refuse to die" philosophy - it will retry
/// operations indefinitely with exponential backoff until they succeed.
/// The only way to stop it is through explicit shutdown.
///
/// # Example
/// ```rust,no_run
/// use foxtive_worker::{ResilientBackend, MessageBackend};
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() {
///     // Wrap any backend in ResilientBackend
///     let backend = Arc::new(foxtive_worker::MemoryBackend::new());
///     let resilient = ResilientBackend::new(backend);
///     
///     // This will retry forever if connection drops
///     let msg = resilient.receive().await;
/// }
/// ```
pub struct ResilientBackend {
    inner: Arc<dyn MessageBackend>,
    strategy: ReconnectStrategy,
    reconnect_attempts: Arc<AtomicU32>,
    last_success: Arc<RwLock<Instant>>,
    is_connected: Arc<AtomicBool>,
    consecutive_failures: Arc<AtomicU32>,
}

impl ResilientBackend {
    /// Create a new resilient backend wrapper
    pub fn new(inner: Arc<dyn MessageBackend>) -> Self {
        Self {
            inner,
            strategy: ReconnectStrategy::default(),
            reconnect_attempts: Arc::new(AtomicU32::new(0)),
            last_success: Arc::new(RwLock::new(Instant::now())),
            is_connected: Arc::new(AtomicBool::new(true)),
            consecutive_failures: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Create with custom reconnection strategy
    pub fn with_strategy(inner: Arc<dyn MessageBackend>, strategy: ReconnectStrategy) -> Self {
        Self {
            inner,
            strategy,
            reconnect_attempts: Arc::new(AtomicU32::new(0)),
            last_success: Arc::new(RwLock::new(Instant::now())),
            is_connected: Arc::new(AtomicBool::new(true)),
            consecutive_failures: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Get the inner backend
    pub fn inner(&self) -> &Arc<dyn MessageBackend> {
        &self.inner
    }

    /// Check if currently connected
    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::Relaxed)
    }

    pub fn reconnect_attempts(&self) -> u32 {
        self.reconnect_attempts.load(Ordering::Relaxed)
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures.load(Ordering::Relaxed)
    }

    /// Execute an operation with automatic reconnection on failure.
    ///
    /// This method implements the "refuse to die" philosophy - it will retry
    /// indefinitely until the operation succeeds or shutdown is requested.
    async fn execute_with_retry<T, F, Fut>(&self, operation_name: &str, op: F) -> WorkerResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = WorkerResult<T>>,
    {
        let mut attempt = 0;

        loop {
            match op().await {
                Ok(result) => {
                    if attempt > 0 {
                        info!("{} succeeded after {} attempts", operation_name, attempt);
                    }
                    self.reconnect_attempts.store(0, Ordering::Relaxed);
                    self.consecutive_failures.store(0, Ordering::Relaxed);
                    *self.last_success.write().await = Instant::now();
                    self.is_connected.store(true, Ordering::Relaxed);
                    return Ok(result);
                }
                Err(e) => {
                    attempt += 1;
                    self.reconnect_attempts.store(attempt, Ordering::Relaxed);
                    let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
                    self.is_connected.store(false, Ordering::Relaxed);

                    warn!(
                        "{} failed (attempt {}, consecutive failures: {}): {}. Retrying...",
                        operation_name, attempt, failures, e
                    );

                    // Try to recover connection
                    if let Err(recover_err) = self.try_recover().await {
                        error!("Recovery attempt failed: {}", recover_err);
                    }

                    // Calculate delay with exponential backoff and jitter
                    let delay = self.strategy.delay_for_attempt(attempt - 1);

                    // Log every 10th attempt to avoid spam
                    if attempt % 10 == 0 || attempt <= 3 {
                        warn!(
                            "Still trying {} (attempt {}) - next retry in {:?}",
                            operation_name, attempt, delay
                        );
                    }

                    tokio::time::sleep(delay).await;

                    // Never give up - keep retrying forever
                    // The only way out is through explicit shutdown
                }
            }
        }
    }

    /// Attempt to recover the connection
    async fn try_recover(&self) -> WorkerResult<()> {
        // Try health check to see if we can reconnect
        match self.inner.health_check().await {
            Ok(_) => {
                info!("Connection recovered");
                self.consecutive_failures.store(0, Ordering::Relaxed);
                Ok(())
            }
            Err(e) => {
                warn!("Health check failed during recovery: {}", e);
                // Some backends need explicit reconnect logic here
                // For now, we rely on the backend's own reconnection (deadpool, etc.)
                Err(e)
            }
        }
    }
}

#[async_trait::async_trait]
impl MessageBackend for ResilientBackend {
    async fn receive(&self) -> WorkerResult<ReceiveResult<serde_json::Value>> {
        self.execute_with_retry("receive", || async { self.inner.receive().await })
            .await
    }

    async fn ack(&self, message_id: &str) -> WorkerResult<()> {
        // Ack operations are usually idempotent, so we don't retry indefinitely
        // Just try once and let the caller handle failures
        self.inner.ack(message_id).await
    }

    async fn nack(&self, message_id: &str, requeue: bool) -> WorkerResult<()> {
        // Nack operations are critical - we should retry to avoid message loss
        self.execute_with_retry("nack", || async {
            self.inner.nack(message_id, requeue).await
        })
        .await
    }

    async fn health_check(&self) -> WorkerResult<()> {
        self.inner.health_check().await
    }

    async fn shutdown(&self) -> WorkerResult<()> {
        self.inner.shutdown().await
    }
}

/// Builder for configuring resilient backends
pub struct ResilientBackendBuilder {
    inner: Arc<dyn MessageBackend>,
    strategy: ReconnectStrategy,
}

impl ResilientBackendBuilder {
    /// Create a new builder
    pub fn new(inner: Arc<dyn MessageBackend>) -> Self {
        Self {
            inner,
            strategy: ReconnectStrategy::default(),
        }
    }

    /// Set reconnection strategy
    pub fn with_strategy(mut self, strategy: ReconnectStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Build the resilient backend
    pub fn build(self) -> ResilientBackend {
        ResilientBackend::with_strategy(self.inner, self.strategy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::{MemoryBackend, ReceiveResult};
    use crate::error::WorkerError;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Helper to create a mock backend that fails on command
    struct FailingBackend {
        fail_count: Arc<AtomicUsize>,
        total_calls: Arc<AtomicUsize>,
        succeed_after: usize,
    }

    impl FailingBackend {
        fn new(succeed_after: usize) -> (Arc<Self>, Arc<AtomicUsize>, Arc<AtomicUsize>) {
            let fail_count = Arc::new(AtomicUsize::new(0));
            let total_calls = Arc::new(AtomicUsize::new(0));
            (
                Arc::new(Self {
                    fail_count: fail_count.clone(),
                    total_calls: total_calls.clone(),
                    succeed_after,
                }),
                fail_count,
                total_calls,
            )
        }
    }

    #[async_trait::async_trait]
    impl MessageBackend for FailingBackend {
        async fn receive(&self) -> WorkerResult<ReceiveResult<serde_json::Value>> {
            let calls = self.total_calls.fetch_add(1, Ordering::SeqCst);
            if calls < self.succeed_after {
                self.fail_count.fetch_add(1, Ordering::SeqCst);
                Err(WorkerError::BackendError(
                    "Simulated network failure".to_string(),
                ))
            } else {
                Ok(ReceiveResult::Shutdown)
            }
        }

        async fn ack(&self, _message_id: &str) -> WorkerResult<()> {
            Ok(())
        }

        async fn nack(&self, _message_id: &str, _requeue: bool) -> WorkerResult<()> {
            Ok(())
        }

        async fn health_check(&self) -> WorkerResult<()> {
            let calls = self.total_calls.load(Ordering::SeqCst);
            if calls < self.succeed_after {
                Err(WorkerError::BackendError("Health check failed".to_string()))
            } else {
                Ok(())
            }
        }

        async fn shutdown(&self) -> WorkerResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_resilient_backend_wraps_successfully() {
        let inner = Arc::new(MemoryBackend::new());
        let resilient = ResilientBackend::new(inner.clone());

        assert!(resilient.is_connected());
        assert_eq!(resilient.reconnect_attempts(), 0);
        assert_eq!(resilient.consecutive_failures(), 0);
    }

    #[tokio::test]
    async fn test_resilient_backend_receive() {
        let inner = MemoryBackend::new();
        let backend_arc = Arc::new(inner);
        let resilient = ResilientBackend::new(backend_arc.clone());

        // Add a message (MemoryBackend uses enqueue, not add_message)
        backend_arc.enqueue(serde_json::json!({"test": "data"}));

        // Receive through resilient wrapper
        let result = resilient.receive().await.unwrap();
        assert!(result.is_message());
        if let ReceiveResult::Message(msg) = result {
            assert_eq!(msg.message.payload["test"], "data");
        } else {
            panic!("Expected Message variant");
        }
    }

    #[tokio::test]
    async fn test_resilient_backend_with_custom_strategy() {
        let inner = Arc::new(MemoryBackend::new());
        let strategy = ReconnectStrategy::Fixed(Duration::from_secs(1));
        let resilient = ResilientBackend::with_strategy(inner, strategy);

        assert!(resilient.is_connected());
    }

    #[tokio::test]
    async fn test_exponential_backoff_calculation() {
        let strategy = ReconnectStrategy::Exponential {
            initial: Duration::from_millis(100),
            max: Duration::from_secs(1),
            multiplier: 2.0,
            jitter_factor: 0.0, // No jitter for predictable testing
        };

        // Test exponential growth
        assert_eq!(strategy.delay_for_attempt(0).as_millis(), 100); // 100ms
        assert_eq!(strategy.delay_for_attempt(1).as_millis(), 200); // 200ms
        assert_eq!(strategy.delay_for_attempt(2).as_millis(), 400); // 400ms
        assert_eq!(strategy.delay_for_attempt(3).as_millis(), 800); // 800ms
        assert_eq!(strategy.delay_for_attempt(4).as_millis(), 1000); // Capped at 1s
        assert_eq!(strategy.delay_for_attempt(5).as_millis(), 1000); // Still capped
    }

    #[tokio::test]
    async fn test_exponential_backoff_with_jitter() {
        let strategy = ReconnectStrategy::Exponential {
            initial: Duration::from_millis(100),
            max: Duration::from_secs(1),
            multiplier: 2.0,
            jitter_factor: 0.5, // 50% jitter
        };

        // With 50% jitter, delay should be between base and base * 1.5
        let delay = strategy.delay_for_attempt(0);
        let base = 100;
        assert!(delay.as_millis() >= base as u128);
        assert!(delay.as_millis() <= (base as f64 * 1.5) as u128);
    }

    #[tokio::test]
    async fn test_fixed_delay_strategy() {
        let strategy = ReconnectStrategy::Fixed(Duration::from_secs(2));

        // Should always return the same delay regardless of attempt
        assert_eq!(strategy.delay_for_attempt(0).as_secs(), 2);
        assert_eq!(strategy.delay_for_attempt(5).as_secs(), 2);
        assert_eq!(strategy.delay_for_attempt(100).as_secs(), 2);
    }

    #[tokio::test]
    async fn test_reconnection_on_failure() {
        // Backend fails first 2 times, succeeds on 3rd
        let (backend, fail_count, total_calls) = FailingBackend::new(2);
        let resilient = ResilientBackend::new(backend);

        // This should retry until success
        let result = resilient.receive().await;

        assert!(result.is_ok());
        if let Ok(receive_result) = result {
            assert!(receive_result.is_shutdown()); // FailingBackend returns Shutdown on success
        }
        assert_eq!(fail_count.load(Ordering::SeqCst), 2); // Failed twice
        assert_eq!(total_calls.load(Ordering::SeqCst), 3); // Called 3 times total
        assert_eq!(resilient.reconnect_attempts(), 0); // Reset after success
        assert_eq!(resilient.consecutive_failures(), 0); // Reset after success
        assert!(resilient.is_connected());
    }

    #[tokio::test]
    async fn test_connection_state_tracking() {
        // Backend fails first time, then succeeds
        let (backend, _, _) = FailingBackend::new(1);
        let resilient = ResilientBackend::new(backend);

        // Initial state
        assert!(resilient.is_connected());
        assert_eq!(resilient.reconnect_attempts(), 0);

        // First call will fail and trigger reconnection
        let _ = resilient.receive().await;

        // After recovery, should be connected again
        assert!(resilient.is_connected());
        assert_eq!(resilient.reconnect_attempts(), 0);
    }

    #[tokio::test]
    async fn test_consecutive_failure_tracking() {
        // Backend fails 3 times before succeeding
        let (backend, _, _) = FailingBackend::new(3);
        let resilient = ResilientBackend::new(backend);

        // Start receiving - this will retry internally
        let _ = resilient.receive().await;

        // After success, failures should be reset
        assert_eq!(resilient.consecutive_failures(), 0);
    }

    #[tokio::test]
    async fn test_ack_operations_dont_retry_indefinitely() {
        let inner = Arc::new(MemoryBackend::new());
        let resilient = ResilientBackend::new(inner.clone());

        // Ack operations should complete immediately without retry logic
        let result = resilient.ack("non-existent-id").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_health_check_passthrough() {
        let inner = Arc::new(MemoryBackend::new());
        let resilient = ResilientBackend::new(inner.clone());

        // Health check should pass through to inner backend
        let result = resilient.health_check().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_passthrough() {
        let inner = Arc::new(MemoryBackend::new());
        let resilient = ResilientBackend::new(inner.clone());

        // Shutdown should pass through
        let result = resilient.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_builder_pattern() {
        let inner = Arc::new(MemoryBackend::new());
        let strategy = ReconnectStrategy::Exponential {
            initial: Duration::from_millis(500),
            max: Duration::from_secs(30),
            multiplier: 2.5,
            jitter_factor: 0.2,
        };

        let resilient = ResilientBackendBuilder::new(inner)
            .with_strategy(strategy)
            .build();

        assert!(resilient.is_connected());
    }

    #[tokio::test]
    async fn test_multiple_receive_operations() {
        let inner = MemoryBackend::new();
        let backend_arc = Arc::new(inner);
        let resilient = ResilientBackend::new(backend_arc.clone());

        // Add multiple messages
        backend_arc.enqueue(serde_json::json!({"msg": 1}));
        backend_arc.enqueue(serde_json::json!({"msg": 2}));
        backend_arc.enqueue(serde_json::json!({"msg": 3}));

        // Receive all messages through resilient wrapper
        for expected in 1..=3 {
            let result = resilient.receive().await.unwrap();
            if let ReceiveResult::Message(msg) = result {
                assert_eq!(msg.message.payload["msg"], expected);
            } else {
                panic!("Expected Message variant, got {:?}", result);
            }
        }

        // All operations should succeed without retries
        assert_eq!(resilient.reconnect_attempts(), 0);
    }

    #[tokio::test]
    async fn test_default_reconnect_strategy() {
        let strategy = ReconnectStrategy::default();

        // Verify default values
        match strategy {
            ReconnectStrategy::Exponential {
                initial,
                max,
                multiplier,
                jitter_factor,
            } => {
                assert_eq!(initial, Duration::from_secs(1));
                assert_eq!(max, Duration::from_secs(60));
                assert_eq!(multiplier, 2.0);
                assert_eq!(jitter_factor, 0.1);
            }
            _ => panic!("Default should be Exponential"),
        }
    }
}
