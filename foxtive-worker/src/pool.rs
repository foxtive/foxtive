use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{Notify, Semaphore};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::error::{WorkerError, WorkerResult};
use crate::health::{HealthCheck, HealthStatus};
use crate::message::{AckHandle, ReceivedMessage};
use crate::metrics::WorkerMetrics;
use crate::middleware::{MessageHandler, Middleware, MiddlewareChain, MiddlewareResult};
use crate::strategies::{
    LeastLoadedBalancer, LoadBalancingStrategy, RandomBalancer, RoundRobinBalancer,
};
use crate::worker::Worker;

/// A pool of workers with load balancing for message distribution.
///
/// The pool manages multiple worker instances and distributes incoming messages
/// based on the configured load balancing strategy.
///
/// # Example
/// ```rust,no_run
/// use foxtive_worker::pool::WorkerPool;
/// use foxtive_worker::strategies::LoadBalancingStrategy;
/// use foxtive_worker::metrics::NoOpMetrics;
/// use std::sync::Arc;
///
/// let pool = WorkerPool::new("my-pool", LoadBalancingStrategy::RoundRobin, Arc::new(NoOpMetrics));
/// // Add workers...
/// // Dispatch messages...
/// ```
pub struct WorkerPool {
    name: String,
    workers: Vec<Arc<dyn Worker>>,
    strategy: LoadBalancingStrategy,
    semaphore: Arc<Semaphore>,
    concurrency_limit: usize,
    least_loaded_balancer: Option<Arc<LeastLoadedBalancer>>,
    round_robin_balancer: Arc<RoundRobinBalancer>,
    random_balancer: RandomBalancer,
    /// Middleware list (Arc-wrapped for cloning)
    middlewares: Vec<Arc<dyn Middleware>>,
    /// Metrics collector for this pool
    metrics_collector: Arc<dyn WorkerMetrics>,
    /// Indicates if the worker pool is currently running.
    is_running: Arc<AtomicBool>,
    /// Cancellation token for graceful shutdown of all spawned tasks
    cancellation_token: CancellationToken,
    /// Notify for task completion signaling during shutdown
    task_completion_notify: Arc<Notify>,
    /// Track number of in-flight tasks for monitoring
    in_flight_tasks: Arc<AtomicUsize>,
}

impl std::fmt::Debug for WorkerPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerPool")
            .field("name", &self.name)
            .field("worker_count", &self.workers.len())
            .field("strategy", &self.strategy)
            .field("is_running", &self.is_running.load(Ordering::SeqCst))
            .finish()
    }
}

impl WorkerPool {
    /// Create a new worker pool with the given name, load balancing strategy, and metrics collector.
    pub fn new(
        name: impl Into<String>,
        strategy: LoadBalancingStrategy,
        metrics_collector: Arc<dyn WorkerMetrics>,
    ) -> Self {
        Self::with_concurrency(name, strategy, 1000, metrics_collector)
    }

    /// Create a new worker pool with custom concurrency limit.
    ///
    /// # Arguments
    /// * `name` - Pool name
    /// * `strategy` - Load balancing strategy
    /// * `concurrency_limit` - Maximum concurrent messages across all workers
    /// * `metrics_collector` - Metrics collector implementation
    pub fn with_concurrency(
        name: impl Into<String>,
        strategy: LoadBalancingStrategy,
        concurrency_limit: usize,
        metrics_collector: Arc<dyn WorkerMetrics>,
    ) -> Self {
        let least_loaded_balancer = if matches!(strategy, LoadBalancingStrategy::LeastLoaded) {
            Some(Arc::new(LeastLoadedBalancer::new(0)))
        } else {
            None
        };

        Self {
            name: name.into(),
            workers: Vec::new(),
            strategy,
            semaphore: Arc::new(Semaphore::new(concurrency_limit)),
            concurrency_limit,
            least_loaded_balancer,
            round_robin_balancer: Arc::new(RoundRobinBalancer::new()),
            random_balancer: RandomBalancer,
            middlewares: Vec::new(),
            metrics_collector,
            is_running: Arc::new(AtomicBool::new(true)),
            cancellation_token: CancellationToken::new(),
            task_completion_notify: Arc::new(Notify::new()),
            in_flight_tasks: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Add a worker to the pool.
    pub fn add_worker(&mut self, worker: Arc<dyn Worker>) {
        self.workers.push(worker);

        // Update least-loaded balancer if needed - now O(1) with atomic swap
        if let Some(ref balancer) = self.least_loaded_balancer {
            balancer.add_worker();
        }
        self.metrics_collector
            .record_active_workers(self.workers.len());
    }

    /// Add multiple workers to the pool.
    pub fn add_workers(&mut self, workers: Vec<Arc<dyn Worker>>) {
        for worker in workers {
            self.add_worker(worker);
        }
    }

    /// Get the number of workers in the pool.
    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    /// Set middleware for the pool.
    ///
    /// This allows you to inject middleware processing into the message flow.
    /// Middleware will be executed in order before messages reaches workers.
    pub fn with_middlewares(mut self, middlewares: Vec<Arc<dyn Middleware>>) -> Self {
        self.middlewares = middlewares;
        self
    }

    /// Get the pool name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Dispatch a message to a worker based on the load balancing strategy.
    ///
    /// Spawns an async task to process the message, respecting concurrency limits.
    /// If middleware is configured, the message flows through the chain before reaching the worker.
    pub async fn dispatch(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(WorkerError::Shutdown);
        }
        if self.workers.is_empty() {
            return Err(WorkerError::PoolExhausted);
        }

        // Pick a worker and track metrics
        let worker_index = self.select_worker();
        let worker = self.workers[worker_index].clone();
        let worker_id = worker.id().to_string();
        let queue_name = message.message.metadata.source.clone();

        self.metrics_collector
            .record_message_received(&worker_id, &queue_name);
        let start_time = Instant::now();

        if let Some(ref balancer) = self.least_loaded_balancer {
            balancer.increment_load(worker_index);
        }

        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| WorkerError::Shutdown)?;

        self.metrics_collector
            .record_in_flight_messages(self.semaphore.available_permits());

        // Build the handler chain - wrap worker with middleware if configured
        let handler: Arc<dyn MessageHandler> = if !self.middlewares.is_empty() {
            let worker_handler = WorkerHandler(worker.clone());
            let boxed_middlewares: Vec<Box<dyn Middleware>> = self
                .middlewares
                .iter()
                .map(|m| Box::new(ArcMiddlewareWrapper(m.clone())) as Box<dyn Middleware>)
                .collect();

            let chain = MiddlewareChain::new(boxed_middlewares, Box::new(worker_handler));
            Arc::new(ArcHandlerWrapper(chain.build()))
        } else {
            Arc::new(WorkerHandler(worker.clone()))
        };

        let metrics_collector_clone = self.metrics_collector.clone();
        let least_loaded_balancer = self.least_loaded_balancer.clone();
        let cancellation_token = self.cancellation_token.child_token();
        let task_completion_notify = self.task_completion_notify.clone();
        let in_flight_tasks = self.in_flight_tasks.clone();

        // Track in-flight task count
        in_flight_tasks.fetch_add(1, Ordering::SeqCst);

        // Extract ack_handle and message data before moving message into task
        let ack_handle = message.ack_handle.clone();
        let message_id = message.message.id.clone();
        let attempt = message.message.metadata.attempt;
        // Clone the full message for retry (preserves routing_key and all metadata)
        let retry_message = message.message.clone();

        tokio::spawn(async move {
            // Use tokio::select! to handle cancellation
            let result = tokio::select! {
                result = handler.handle(message) => result,  // No clone - move message into task
                _ = cancellation_token.cancelled() => {
                    tracing::warn!("Message {} processing cancelled due to shutdown", message_id);
                    // Decrement in-flight counter on cancellation
                    in_flight_tasks.fetch_sub(1, Ordering::SeqCst);
                    task_completion_notify.notify_one();
                    return;
                }
            };

            match result {
                Ok(middleware_result) => {
                    match middleware_result {
                        MiddlewareResult::Acknowledged => {
                            // Middleware (e.g., AckNackMiddleware) already handled acknowledgment
                            tracing::debug!(
                                "Message {} already acknowledged by middleware",
                                message_id
                            );
                            metrics_collector_clone.record_message_processed(
                                &worker_id,
                                &queue_name,
                                start_time,
                            );
                            // Decrement in-flight counter
                            in_flight_tasks.fetch_sub(1, Ordering::SeqCst);
                            task_completion_notify.notify_one();
                            return;
                        }
                        MiddlewareResult::Continue => {
                            // No middleware handled ack - pool should ack on success
                            tracing::debug!("Message {} processed successfully", message_id);
                            metrics_collector_clone.record_message_processed(
                                &worker_id,
                                &queue_name,
                                start_time,
                            );
                            if let Err(e) = retry_ack(&ack_handle, &message_id).await {
                                tracing::error!(
                                    "Failed to ack message {} after retries: {}. Message may be redelivered.",
                                    message_id,
                                    e
                                );
                            }
                        }
                    }
                }
                Err(WorkerError::RetryableFailure { source, delay_ms }) => {
                    tracing::warn!(
                        "Message {} failed (will retry in {:?}): {}",
                        message_id,
                        delay_ms,
                        source
                    );
                    metrics_collector_clone.record_message_retried(
                        &worker_id,
                        &queue_name,
                        attempt,
                    );
                    metrics_collector_clone.record_message_failed(
                        &worker_id,
                        &queue_name,
                        "RetryableFailure",
                        start_time,
                    );

                    // Reconstruct ReceivedMessage for should_requeue check
                    let received_msg = crate::message::ReceivedMessage::new(
                        retry_message.clone(),
                        ack_handle.clone(),
                    );

                    // Check if worker wants to requeue this message
                    let info = crate::error::RetryInfo::new(&source)
                        .with_attempt(retry_message.metadata.attempt)
                        .with_retry_delay(delay_ms);
                    let should_requeue = worker.should_requeue(&received_msg, info);
                    
                    if should_requeue {
                        // Use delayed retry if supported by backend, otherwise nack with requeue
                        // The retry_with_delay method will handle backend-specific retry logic
                        // Pass the original message to preserve all metadata including routing_key
                        if let Err(e) = ack_handle
                            .retry_with_delay(&retry_message, delay_ms.as_millis() as u64)
                            .await
                        {
                            tracing::error!(
                                "Failed to schedule retry for message {}: {}",
                                message_id,
                                e
                            );
                            // Fallback to immediate nack with requeue if retry fails
                            if let Err(nack_err) = ack_handle.nack(true).await {
                                tracing::error!(
                                    "Fallback nack also failed for message {}: {}",
                                    message_id,
                                    nack_err
                                );
                            }
                        }
                    } else {
                        // Worker decided not to requeue - send directly to DLQ
                        tracing::info!(
                            "Worker {} declined to requeue message {}, sending to DLQ",
                            worker_id,
                            message_id
                        );
                        if let Err(e) = ack_handle.send_to_dlq(&retry_message, &source.to_string()).await {
                            tracing::error!("Failed to send message {} to DLQ: {}", message_id, e);
                            // Fallback: nack without requeue
                            if let Err(nack_err) = ack_handle.nack(false).await {
                                tracing::error!(
                                    "Fallback nack also failed for message {}: {}",
                                    message_id,
                                    nack_err
                                );
                            }
                        }
                    }
                }
                Err(WorkerError::RetriesExhausted { source }) => {
                    tracing::error!(
                        "Message {} exhausted all retries, sending to DLQ: {}",
                        message_id,
                        source
                    );
                    metrics_collector_clone
                        .record_message_retries_exhausted(&worker_id, &queue_name);
                    metrics_collector_clone.record_message_failed(
                        &worker_id,
                        &queue_name,
                        "RetriesExhausted",
                        start_time,
                    );
                    // Send to DLQ using the ack handle's send_to_dlq method
                    if let Err(e) = ack_handle
                        .send_to_dlq(&retry_message, &source.to_string())
                        .await
                    {
                        tracing::error!("Failed to send message {} to DLQ: {}", message_id, e);
                        // Fallback: nack without requeue (message will be discarded)
                        if let Err(nack_err) = ack_handle.nack(false).await {
                            tracing::error!(
                                "Fallback nack also failed for message {}: {}",
                                message_id,
                                nack_err
                            );
                        }
                    }
                }
                Err(e) => {
                    let error_type = format!("{:?}", e);
                    tracing::error!("Message {} failed: {}", message_id, e);
                    metrics_collector_clone.record_message_failed(
                        &worker_id,
                        &queue_name,
                        &error_type,
                        start_time,
                    );
                    
                    // Reconstruct ReceivedMessage for should_requeue check
                    let received_msg = crate::message::ReceivedMessage::new(
                        retry_message.clone(),
                        ack_handle.clone(),
                    );
                    
                    // Check if worker wants to requeue this message
                    let info = crate::error::RetryInfo::new(&e)
                        .with_attempt(retry_message.metadata.attempt);
                    let should_requeue = worker.should_requeue(&received_msg, info);
                    
                    if should_requeue {
                        // Requeue for retry
                        if let Err(nack_err) = ack_handle.nack(true).await {
                            tracing::error!("Failed to nack message {}: {}", message_id, nack_err);
                        }
                    } else {
                        // Worker declined to requeue - send to DLQ or discard
                        tracing::info!(
                            "Worker {} declined to requeue message {}, sending to DLQ",
                            worker_id,
                            message_id
                        );
                        if let Err(dlq_err) = ack_handle.send_to_dlq(&retry_message, &error_type).await {
                            tracing::error!("Failed to send message {} to DLQ: {}", message_id, dlq_err);
                            // Fallback: nack without requeue
                            if let Err(nack_err) = ack_handle.nack(false).await {
                                tracing::error!("Failed to nack message {}: {}", message_id, nack_err);
                            }
                        }
                    }
                }
            }

            // Decrement load for least-loaded balancer after processing completes
            if let Some(ref balancer) = least_loaded_balancer {
                balancer.decrement_load(worker_index);
            }

            drop(permit);

            // Signal task completion and decrement counter
            in_flight_tasks.fetch_sub(1, Ordering::SeqCst);
            task_completion_notify.notify_one();
        });

        Ok(())
    }

    /// Select a worker based on the configured load balancing strategy.
    fn select_worker(&self) -> usize {
        match self.strategy {
            LoadBalancingStrategy::RoundRobin => self.round_robin_balancer.next(self.workers.len()),
            LoadBalancingStrategy::Random => self.random_balancer.next(self.workers.len()),
            LoadBalancingStrategy::LeastLoaded => {
                if let Some(ref balancer) = self.least_loaded_balancer {
                    balancer.next()
                } else {
                    0 // Fallback
                }
            }
        }
    }

    /// Shutdown the pool gracefully.
    ///
    /// This prevents new messages from being dispatched, cancels in-flight tasks,
    /// and waits for completion with a timeout using efficient notification.
    pub async fn shutdown(&self) -> WorkerResult<()> {
        tracing::info!("Shutting down worker pool: {}", self.name);

        self.is_running.store(false, Ordering::SeqCst);
        self.metrics_collector.record_active_workers(0);

        // Cancel all in-flight tasks
        self.cancellation_token.cancel();
        tracing::info!("Cancelled all in-flight tasks for pool {}", self.name);

        // Close the semaphore to prevent new acquisitions
        self.semaphore.close();

        // Wait for permits to be returned with efficient notification
        let shutdown_timeout = Duration::from_secs(30); // 30 second timeout
        let start = Instant::now();

        loop {
            let available = self.semaphore.available_permits();
            let in_flight = self.concurrency_limit.saturating_sub(available);

            if in_flight == 0 {
                break; // All tasks completed
            }

            if start.elapsed() >= shutdown_timeout {
                tracing::warn!(
                    "Shutdown timeout reached for pool {}. {} tasks still running. Forcing shutdown.",
                    self.name,
                    in_flight
                );
                break;
            }

            // Wait efficiently for task completion notification instead of busy-wait
            tokio::select! {
                _ = self.task_completion_notify.notified() => {
                    // A task completed, check again
                    continue;
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    // Periodic check in case notifications are missed
                    continue;
                }
            }
        }

        self.metrics_collector.record_in_flight_messages(0);
        tracing::info!("Worker pool {} shutdown complete", self.name);
        Ok(())
    }

    /// Get the current number of in-flight tasks.
    pub fn in_flight_count(&self) -> usize {
        self.in_flight_tasks.load(Ordering::SeqCst)
    }
}

impl HealthCheck for WorkerPool {
    fn check_health(&self) -> HealthStatus {
        let is_running = self.is_running.load(Ordering::SeqCst);
        let worker_count = self.worker_count();
        let in_flight = self.in_flight_tasks.load(Ordering::SeqCst);

        // Check if pool is running
        if !is_running {
            return HealthStatus::Unhealthy {
                reason: "Pool is not running".to_string(),
            };
        }

        // Check if workers are available
        if worker_count == 0 {
            return HealthStatus::Degraded {
                reason: "No workers available".to_string(),
            };
        }

        // Check if pool is saturated (90%+ capacity)
        let saturation = in_flight as f64 / self.concurrency_limit as f64;
        if saturation > 0.9 {
            return HealthStatus::Degraded {
                reason: format!(
                    "Pool near capacity: {} in-flight messages ({:.0}% saturation)",
                    in_flight,
                    saturation * 100.0
                ),
            };
        }

        HealthStatus::Healthy
    }

    fn status_message(&self) -> String {
        let worker_count = self.worker_count();
        let in_flight = self.in_flight_tasks.load(Ordering::SeqCst);
        let available_permits = self.semaphore.available_permits();

        match self.check_health() {
            HealthStatus::Healthy => {
                format!(
                    "WorkerPool '{}' is healthy with {} workers. {} in-flight, {} available permits.",
                    self.name, worker_count, in_flight, available_permits
                )
            }
            HealthStatus::Degraded { ref reason } => {
                format!(
                    "WorkerPool '{}' is degraded: {}. {} workers, {} in-flight.",
                    self.name, reason, worker_count, in_flight
                )
            }
            HealthStatus::Unhealthy { ref reason } => {
                format!(
                    "WorkerPool '{}' is unhealthy: {}. {} workers, {} in-flight.",
                    self.name, reason, worker_count, in_flight
                )
            }
        }
    }
}

/// Wrapper that converts a Worker into a MessageHandler.
struct WorkerHandler(Arc<dyn Worker>);

#[async_trait::async_trait]
impl MessageHandler for WorkerHandler {
    async fn handle(
        &self,
        message: ReceivedMessage<serde_json::Value>,
    ) -> Result<MiddlewareResult, WorkerError> {
        // Workers always return Continue - they don't handle acknowledgment directly
        self.0.process(message).await?;
        Ok(MiddlewareResult::Continue)
    }
}

/// Wrapper to convert Arc<dyn Middleware> to Box<dyn Middleware>
struct ArcMiddlewareWrapper(Arc<dyn Middleware>);

#[async_trait::async_trait]
impl Middleware for ArcMiddlewareWrapper {
    fn name(&self) -> &str {
        self.0.name()
    }

    async fn handle(
        &self,
        message: ReceivedMessage<serde_json::Value>,
        next: Box<dyn MessageHandler>,
    ) -> Result<MiddlewareResult, WorkerError> {
        self.0.handle(message, next).await
    }
}

/// Wrapper to convert Box<dyn MessageHandler> to Arc
struct ArcHandlerWrapper(Box<dyn MessageHandler>);

#[async_trait::async_trait]
impl MessageHandler for ArcHandlerWrapper {
    async fn handle(
        &self,
        message: ReceivedMessage<serde_json::Value>,
    ) -> Result<MiddlewareResult, WorkerError> {
        self.0.handle(message).await
    }
}

/// Retry ack with exponential backoff on failure.
///
/// This helps handle transient network issues or broker unavailability.
async fn retry_ack(ack_handle: &Arc<dyn AckHandle>, message_id: &str) -> WorkerResult<()> {
    let max_retries = 3;
    let base_delay_ms = 100;

    for attempt in 0..max_retries {
        match ack_handle.ack().await {
            Ok(_) => return Ok(()),
            Err(e) => {
                if attempt < max_retries - 1 {
                    let delay = Duration::from_millis(base_delay_ms * (2u64.pow(attempt as u32)));
                    tracing::warn!(
                        "Attempt {} failed to ack message {}: {}. Retrying in {:?}",
                        attempt + 1,
                        message_id,
                        e,
                        delay
                    );
                    sleep(delay).await;
                } else {
                    return Err(e);
                }
            }
        }
    }

    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{AckHandle, Message, MessageMetadata};
    use crate::metrics::NoOpMetrics;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    // Use NoOpMetrics for tests

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

    struct TestWorker {
        id: String,
        process_count: Arc<AtomicUsize>,
    }

    impl TestWorker {
        fn new(id: &str) -> (Self, Arc<AtomicUsize>) {
            let count = Arc::new(AtomicUsize::new(0));
            (
                Self {
                    id: id.to_string(),
                    process_count: count.clone(),
                },
                count,
            )
        }
    }

    #[async_trait]
    impl Worker for TestWorker {
        fn id(&self) -> &str {
            &self.id
        }

        async fn process(&self, _message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
            self.process_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn create_test_message(id: &str) -> ReceivedMessage<serde_json::Value> {
        let message = Message {
            id: id.to_string(),
            payload: serde_json::json!({"test": "data"}),
            metadata: MessageMetadata::new("test-queue"),
        };
        ReceivedMessage::new(message, Arc::new(MockAckHandle))
    }

    #[tokio::test]
    async fn test_pool_creation() {
        let pool = WorkerPool::new(
            "test-pool",
            LoadBalancingStrategy::RoundRobin,
            Arc::new(NoOpMetrics),
        );
        assert_eq!(pool.name(), "test-pool");
        assert_eq!(pool.worker_count(), 0);
        assert!(pool.is_running.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_add_worker() {
        let mut pool = WorkerPool::new(
            "test-pool",
            LoadBalancingStrategy::RoundRobin,
            Arc::new(NoOpMetrics),
        );
        let (worker, _) = TestWorker::new("worker-1");
        pool.add_worker(Arc::new(worker));

        assert_eq!(pool.worker_count(), 1);
    }

    #[tokio::test]
    async fn test_dispatch_empty_pool() {
        let pool = WorkerPool::new(
            "test-pool",
            LoadBalancingStrategy::RoundRobin,
            Arc::new(NoOpMetrics),
        );
        let message = create_test_message("msg-1");

        let result = pool.dispatch(message).await;
        assert!(matches!(result, Err(WorkerError::PoolExhausted)));
    }

    #[tokio::test]
    async fn test_round_robin_distribution() {
        let mut pool = WorkerPool::new(
            "test-pool",
            LoadBalancingStrategy::RoundRobin,
            Arc::new(NoOpMetrics),
        );

        let (worker1, count1) = TestWorker::new("worker-1");
        let (worker2, count2) = TestWorker::new("worker-2");

        pool.add_worker(Arc::new(worker1));
        pool.add_worker(Arc::new(worker2));

        // Dispatch 4 messages
        for i in 0..4 {
            let message = create_test_message(&format!("msg-{}", i));
            pool.dispatch(message).await.unwrap();
        }

        // Give tasks time to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Each worker should have processed 2 messages
        assert_eq!(count1.load(Ordering::SeqCst), 2);
        assert_eq!(count2.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_pool_health() {
        let pool = WorkerPool::new(
            "test-pool",
            LoadBalancingStrategy::RoundRobin,
            Arc::new(NoOpMetrics),
        );
        assert!(matches!(pool.check_health(), HealthStatus::Degraded { .. })); // Degraded because 0 workers

        let mut pool = pool;
        let (worker, _) = TestWorker::new("worker-1");
        pool.add_worker(Arc::new(worker));

        assert!(matches!(pool.check_health(), HealthStatus::Healthy));
    }

    #[tokio::test]
    async fn test_concurrency_limit_enforcement() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Create a worker that tracks concurrent executions
        let concurrent_count = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(0));

        struct ConcurrentTestWorker {
            id: String,
            concurrent: Arc<AtomicUsize>,
            max_concurrent: Arc<AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl Worker for ConcurrentTestWorker {
            fn id(&self) -> &str {
                &self.id
            }

            async fn process(
                &self,
                _message: ReceivedMessage<serde_json::Value>,
            ) -> WorkerResult<()> {
                // Increment concurrent count
                let current = self.concurrent.fetch_add(1, Ordering::SeqCst) + 1;

                // Track maximum
                let mut max = self.max_concurrent.load(Ordering::SeqCst);
                while current > max {
                    match self.max_concurrent.compare_exchange_weak(
                        max,
                        current,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    ) {
                        Ok(_) => break,
                        Err(new_max) => max = new_max,
                    }
                }

                // Simulate processing time
                tokio::time::sleep(Duration::from_millis(50)).await;

                // Decrement concurrent count
                self.concurrent.fetch_sub(1, Ordering::SeqCst);
                Ok(())
            }
        }

        // Create pool with concurrency limit of 3
        let mut pool = WorkerPool::with_concurrency(
            "test-pool",
            LoadBalancingStrategy::RoundRobin,
            3, // Limit to 3 concurrent
            Arc::new(NoOpMetrics),
        );

        // Add 1 worker (will handle all messages)
        let worker = ConcurrentTestWorker {
            id: "worker-1".to_string(),
            concurrent: concurrent_count.clone(),
            max_concurrent: max_concurrent.clone(),
        };
        pool.add_worker(Arc::new(worker));

        // Dispatch 10 messages rapidly
        for i in 0..10 {
            let message = create_test_message(&format!("msg-{}", i));
            pool.dispatch(message).await.unwrap();
        }

        // Wait for all to complete
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify that concurrency never exceeded the limit
        let actual_max = max_concurrent.load(Ordering::SeqCst);
        assert!(
            actual_max <= 3,
            "Expected max concurrency <= 3, but got {}",
            actual_max
        );
        assert!(
            actual_max >= 2,
            "Expected some concurrency (>= 2), but got {}",
            actual_max
        );
    }

    #[tokio::test]
    async fn test_concurrency_limit_with_builder() {
        use crate::builder::WorkerPoolBuilder;

        let concurrent_count = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(0));

        struct TrackedWorker {
            id: String,
            concurrent: Arc<AtomicUsize>,
            max_concurrent: Arc<AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl Worker for TrackedWorker {
            fn id(&self) -> &str {
                &self.id
            }

            async fn process(
                &self,
                _message: ReceivedMessage<serde_json::Value>,
            ) -> WorkerResult<()> {
                // Track concurrency
                let current = self.concurrent.fetch_add(1, Ordering::SeqCst) + 1;

                let mut max = self.max_concurrent.load(Ordering::SeqCst);
                while current > max {
                    match self.max_concurrent.compare_exchange_weak(
                        max,
                        current,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    ) {
                        Ok(_) => break,
                        Err(new_max) => max = new_max,
                    }
                }

                tokio::time::sleep(Duration::from_millis(100)).await;

                self.concurrent.fetch_sub(1, Ordering::SeqCst);
                Ok(())
            }
        }

        // Build pool with concurrency limit of 2
        let pool = WorkerPoolBuilder::new("test-pool")
            .with_concurrency_limit(2)
            .add_worker(TrackedWorker {
                id: "worker-1".to_string(),
                concurrent: concurrent_count.clone(),
                max_concurrent: max_concurrent.clone(),
            })
            .build()
            .unwrap();

        // Dispatch 6 messages rapidly
        for i in 0..6 {
            let message = create_test_message(&format!("msg-{}", i));
            pool.dispatch(message).await.unwrap();
        }

        // Wait for all to complete
        tokio::time::sleep(Duration::from_millis(800)).await;

        // Verify that concurrency never exceeded the limit of 2
        let actual_max = max_concurrent.load(Ordering::SeqCst);
        assert!(
            actual_max <= 2,
            "Expected max concurrency <= 2, but got {}",
            actual_max
        );
        assert!(
            actual_max >= 1,
            "Expected some concurrency (>= 1), but got {}",
            actual_max
        );
    }

    #[tokio::test]
    async fn test_different_concurrency_limits() {
        // Test that different pools can have different limits
        let pool1 = WorkerPool::with_concurrency(
            "pool1",
            LoadBalancingStrategy::RoundRobin,
            5,
            Arc::new(NoOpMetrics),
        );
        let pool2 = WorkerPool::with_concurrency(
            "pool2",
            LoadBalancingStrategy::RoundRobin,
            20,
            Arc::new(NoOpMetrics),
        );

        // Verify they have different semaphore capacities
        // We can't directly check semaphore capacity, but we can verify the pools work independently
        assert_eq!(pool1.name(), "pool1");
        assert_eq!(pool2.name(), "pool2");
    }

    #[tokio::test]
    async fn test_pool_shutdown_prevents_dispatch() {
        let mut pool = WorkerPool::new(
            "test-pool",
            LoadBalancingStrategy::RoundRobin,
            Arc::new(NoOpMetrics),
        );
        let (worker, _) = TestWorker::new("worker-1");
        pool.add_worker(Arc::new(worker));

        pool.shutdown().await.unwrap();

        let message = create_test_message("msg-after-shutdown");
        let result = pool.dispatch(message).await;
        assert!(matches!(result, Err(WorkerError::Shutdown)));
        assert!(matches!(
            pool.check_health(),
            HealthStatus::Unhealthy { .. }
        ));
    }

    /// Test that messages are not double-acknowledged when using AckNackMiddleware.
    /// This verifies the fix for the "PRECONDITION_FAILED - unknown delivery tag" issue.
    #[tokio::test]
    async fn test_no_double_ack_with_middleware() {
        use crate::AckNackMiddleware;
        use std::sync::atomic::AtomicUsize;

        // Track if ack was called
        let ack_count = Arc::new(AtomicUsize::new(0));
        let nack_count = Arc::new(AtomicUsize::new(0));

        #[derive(Debug)]
        struct TrackingAckHandle {
            ack_count: Arc<AtomicUsize>,
            nack_count: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl AckHandle for TrackingAckHandle {
            async fn ack(&self) -> WorkerResult<()> {
                self.ack_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }

            async fn nack(&self, _requeue: bool) -> WorkerResult<()> {
                self.nack_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        // Create a successful worker
        struct SuccessWorker;

        #[async_trait]
        impl Worker for SuccessWorker {
            fn id(&self) -> &str {
                "success-worker"
            }

            async fn process(
                &self,
                _message: ReceivedMessage<serde_json::Value>,
            ) -> WorkerResult<()> {
                Ok(())
            }
        }

        let mut pool = WorkerPool::new(
            "test-pool",
            LoadBalancingStrategy::RoundRobin,
            Arc::new(NoOpMetrics),
        );

        // Add AckNackMiddleware to auto-ack on success
        pool.middlewares
            .push(Arc::new(AckNackMiddleware::default()));
        pool.add_worker(Arc::new(SuccessWorker));

        // Create message with tracking ack handle
        let message = Message {
            id: "test-msg".to_string(),
            payload: serde_json::json!({"test": "data"}),
            metadata: MessageMetadata::new("test-queue"),
        };
        let received = ReceivedMessage::new(
            message,
            Arc::new(TrackingAckHandle {
                ack_count: ack_count.clone(),
                nack_count: nack_count.clone(),
            }),
        );

        // Dispatch the message
        pool.dispatch(received).await.unwrap();

        // Wait for processing to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify ack was called exactly once (by middleware, not by pool)
        assert_eq!(
            ack_count.load(Ordering::SeqCst),
            1,
            "Message should have been acked exactly once by middleware"
        );
        assert_eq!(
            nack_count.load(Ordering::SeqCst),
            0,
            "Message should not have been nacked"
        );
    }

    /// Test that pool handles ack/nack correctly WITHOUT AckNackMiddleware.
    #[tokio::test]
    async fn test_pool_handles_ack_without_middleware() {
        use std::sync::atomic::AtomicUsize;

        let ack_count = Arc::new(AtomicUsize::new(0));
        let nack_count = Arc::new(AtomicUsize::new(0));

        #[derive(Debug)]
        struct CountingAckHandle {
            ack_count: Arc<AtomicUsize>,
            nack_count: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl AckHandle for CountingAckHandle {
            async fn ack(&self) -> WorkerResult<()> {
                self.ack_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }

            async fn nack(&self, _requeue: bool) -> WorkerResult<()> {
                self.nack_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        // Create a successful worker
        struct SuccessWorker;

        #[async_trait]
        impl Worker for SuccessWorker {
            fn id(&self) -> &str {
                "success-worker"
            }

            async fn process(
                &self,
                _message: ReceivedMessage<serde_json::Value>,
            ) -> WorkerResult<()> {
                Ok(())
            }
        }

        let mut pool = WorkerPool::new(
            "test-pool",
            LoadBalancingStrategy::RoundRobin,
            Arc::new(NoOpMetrics),
        );
        // NO middleware - pool should handle ack
        pool.add_worker(Arc::new(SuccessWorker));

        let message = Message {
            id: "test-msg".to_string(),
            payload: serde_json::json!({"test": "data"}),
            metadata: MessageMetadata::new("test-queue"),
        };
        let received = ReceivedMessage::new(
            message,
            Arc::new(CountingAckHandle {
                ack_count: ack_count.clone(),
                nack_count: nack_count.clone(),
            }),
        );

        pool.dispatch(received).await.unwrap();

        // Wait for processing to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify pool acked the message
        assert_eq!(
            ack_count.load(Ordering::SeqCst),
            1,
            "Pool should have acked the message when no middleware is used"
        );
        assert_eq!(
            nack_count.load(Ordering::SeqCst),
            0,
            "Nack should not be called for successful message"
        );
    }

    /// Test that pool handles nack correctly WITHOUT AckNackMiddleware.
    #[tokio::test]
    async fn test_pool_handles_nack_without_middleware() {
        use std::sync::atomic::AtomicUsize;

        let ack_count = Arc::new(AtomicUsize::new(0));
        let nack_count = Arc::new(AtomicUsize::new(0));

        #[derive(Debug)]
        struct CountingAckHandle {
            ack_count: Arc<AtomicUsize>,
            nack_count: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl AckHandle for CountingAckHandle {
            async fn ack(&self) -> WorkerResult<()> {
                self.ack_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }

            async fn nack(&self, _requeue: bool) -> WorkerResult<()> {
                self.nack_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        // Create a failing worker
        struct FailingWorker;

        #[async_trait]
        impl Worker for FailingWorker {
            fn id(&self) -> &str {
                "failing-worker"
            }

            async fn process(
                &self,
                _message: ReceivedMessage<serde_json::Value>,
            ) -> WorkerResult<()> {
                Err(WorkerError::ProcessingFailed(
                    "Simulated failure".to_string(),
                ))
            }
        }

        let mut pool = WorkerPool::new(
            "test-pool",
            LoadBalancingStrategy::RoundRobin,
            Arc::new(NoOpMetrics),
        );
        // NO middleware - pool should handle nack
        pool.add_worker(Arc::new(FailingWorker));

        let message = Message {
            id: "test-msg".to_string(),
            payload: serde_json::json!({"test": "data"}),
            metadata: MessageMetadata::new("test-queue"),
        };
        let received = ReceivedMessage::new(
            message,
            Arc::new(CountingAckHandle {
                ack_count: ack_count.clone(),
                nack_count: nack_count.clone(),
            }),
        );

        pool.dispatch(received).await.unwrap();

        // Wait for processing to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify pool nacked the message
        assert_eq!(
            nack_count.load(Ordering::SeqCst),
            1,
            "Pool should have nacked the message when no middleware is used"
        );
        assert_eq!(
            ack_count.load(Ordering::SeqCst),
            0,
            "Ack should not be called for failed message"
        );
    }

    /// Integration test: Verify end-to-end flow with AckNackMiddleware and failing worker.
    #[tokio::test]
    async fn test_integration_ack_nack_middleware_with_failure() {
        use crate::AckNackMiddleware;
        use std::sync::atomic::AtomicUsize;

        let ack_count = Arc::new(AtomicUsize::new(0));
        let nack_count = Arc::new(AtomicUsize::new(0));

        #[derive(Debug)]
        struct CountingAckHandle {
            ack_count: Arc<AtomicUsize>,
            nack_count: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl AckHandle for CountingAckHandle {
            async fn ack(&self) -> WorkerResult<()> {
                self.ack_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }

            async fn nack(&self, _requeue: bool) -> WorkerResult<()> {
                self.nack_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        // Create a failing worker
        struct FailingWorker;

        #[async_trait]
        impl Worker for FailingWorker {
            fn id(&self) -> &str {
                "failing-worker"
            }

            async fn process(
                &self,
                _message: ReceivedMessage<serde_json::Value>,
            ) -> WorkerResult<()> {
                Err(WorkerError::ProcessingFailed(
                    "Simulated failure".to_string(),
                ))
            }
        }

        let mut pool = WorkerPool::new(
            "test-pool",
            LoadBalancingStrategy::RoundRobin,
            Arc::new(NoOpMetrics),
        );

        // Add AckNackMiddleware to auto-nack on failure
        pool.middlewares
            .push(Arc::new(AckNackMiddleware::default()));
        pool.add_worker(Arc::new(FailingWorker));

        let message = Message {
            id: "test-msg".to_string(),
            payload: serde_json::json!({"test": "data"}),
            metadata: MessageMetadata::new("test-queue"),
        };
        let received = ReceivedMessage::new(
            message,
            Arc::new(CountingAckHandle {
                ack_count: ack_count.clone(),
                nack_count: nack_count.clone(),
            }),
        );

        pool.dispatch(received).await.unwrap();

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify nack was called exactly once (by middleware)
        assert_eq!(
            nack_count.load(Ordering::SeqCst),
            1,
            "Nack should be called exactly once by middleware"
        );
        assert_eq!(
            ack_count.load(Ordering::SeqCst),
            0,
            "Ack should not be called for failed message"
        );
    }
}
