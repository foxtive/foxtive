use std::sync::Arc;

use crate::error::{WorkerError, WorkerResult};
use crate::message::ReceivedMessage;
use crate::middleware::{MessageHandler, Middleware};
use crate::pool::WorkerPool;
use crate::strategies::LoadBalancingStrategy;
use crate::worker::Worker;
use crate::metrics::{WorkerMetrics, NoOpMetrics}; // Import NoOpMetrics

/// Builder for configuring and creating worker pools.
///
/// Provides a fluent API for setting up worker pools with custom configurations.
///
/// # Example
/// ```rust,no_run
/// use foxtive_worker::builder::WorkerPoolBuilder;
/// use foxtive_worker::strategies::LoadBalancingStrategy;
/// use foxtive_worker::metrics::NoOpMetrics;
/// use foxtive_worker::{Worker, ReceivedMessage};
/// use foxtive_worker::error::WorkerResult;
/// use std::sync::Arc;
///
/// struct MyWorker;
/// #[async_trait::async_trait]
/// impl Worker for MyWorker {
///     fn id(&self) -> &str { "my-worker" }
///     async fn process(&self, _msg: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
///         Ok(())
///     }
/// }
///
/// let pool = WorkerPoolBuilder::new("my-pool")
///     .with_strategy(LoadBalancingStrategy::RoundRobin)
///     .with_concurrency_limit(100)
///     .with_metrics_collector(Arc::new(NoOpMetrics))
///     .add_worker(MyWorker)
///     .build();
/// ```
pub struct WorkerPoolBuilder {
    name: String,
    strategy: LoadBalancingStrategy,
    concurrency_limit: usize,
    workers: Vec<Arc<dyn Worker>>,
    middlewares: Vec<Arc<dyn Middleware>>,
    metrics_collector: Option<Arc<dyn WorkerMetrics>>,
}

impl WorkerPoolBuilder {
    /// Create a new builder with the given pool name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            strategy: LoadBalancingStrategy::default(),
            concurrency_limit: 1000,
            workers: Vec::new(),
            middlewares: Vec::new(),
            metrics_collector: None,
        }
    }

    /// Set the load balancing strategy.
    ///
    /// # Arguments
    /// * `strategy` - The strategy to use for distributing messages
    pub fn with_strategy(mut self, strategy: LoadBalancingStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Set the concurrency limit for the pool.
    ///
    /// This limits how many messages can be processed concurrently across all workers.
    ///
    /// # Arguments
    /// * `limit` - Maximum concurrent messages
    pub fn with_concurrency_limit(mut self, limit: usize) -> Self {
        self.concurrency_limit = limit;
        self
    }

    /// Set the metrics collector for the pool.
    ///
    /// # Arguments
    /// * `collector` - An `Arc` to an object implementing `WorkerMetrics`.
    pub fn with_metrics_collector(mut self, collector: Arc<dyn WorkerMetrics>) -> Self {
        self.metrics_collector = Some(collector);
        self
    }

    /// Add a single worker to the pool.
    ///
    /// # Arguments
    /// * `worker` - The worker to add
    pub fn add_worker<W: Worker + 'static>(mut self, worker: W) -> Self {
        self.workers.push(Arc::new(worker));
        self
    }

    /// Add a boxed worker to the pool.
    ///
    /// Useful when adding heterogeneous workers.
    ///
    /// # Arguments
    /// * `worker` - The boxed worker to add
    pub fn add_boxed_worker(mut self, worker: Box<dyn Worker>) -> Self {
        self.workers.push(worker.into());
        self
    }

    /// Add an Arc-wrapped worker to the pool.
    ///
    /// This is the most efficient way if you already have an Arc.
    ///
    /// # Arguments
    /// * `worker` - The Arc-wrapped worker to add
    pub fn add_arc_worker(mut self, worker: Arc<dyn Worker>) -> Self {
        self.workers.push(worker);
        self
    }

    /// Add multiple workers to the pool.
    ///
    /// # Arguments
    /// * `workers` - Vector of workers to add
    pub fn add_workers<W: Worker + 'static>(mut self, workers: Vec<W>) -> Self {
        for worker in workers {
            self.workers.push(Arc::new(worker));
        }
        self
    }

    /// Add a middleware to the pool.
    ///
    /// Middleware will be executed in the order they are added, forming a chain
    /// that processes messages before they reach the workers.
    ///
    /// # Arguments
    /// * `middleware` - The middleware to add
    pub fn with_middleware<M: Middleware + 'static>(mut self, middleware: M) -> Self {
        self.middlewares.push(Arc::new(middleware));
        self
    }

    /// Add multiple middleware to the pool.
    pub fn with_middlewares(mut self, middlewares: Vec<Arc<dyn Middleware>>) -> Self {
        self.middlewares.extend(middlewares);
        self
    }

    /// Build the worker pool with the configured settings.
    ///
    /// # Returns
    /// A configured `WorkerPool` ready to accept messages
    ///
    /// # Errors
    /// Returns `WorkerError::ConfigError` if:
    /// - No workers were added to the pool
    pub fn build(self) -> WorkerResult<WorkerPool> {
        if self.workers.is_empty() {
            return Err(WorkerError::ConfigError(
                "Cannot build worker pool without any workers".to_string(),
            ));
        }

        let metrics_collector = self.metrics_collector.unwrap_or_else(|| Arc::new(NoOpMetrics));

        let mut pool = WorkerPool::with_concurrency(
            &self.name,
            self.strategy,
            self.concurrency_limit,
            metrics_collector,
        );
        
        // Add workers
        for worker in self.workers {
            pool.add_worker(worker);
        }

        // Pass middleware list to pool (will be used to build dynamic chains at dispatch time)
        if !self.middlewares.is_empty() {
            pool = pool.with_middlewares(self.middlewares);
        }

        Ok(pool)
    }

    /// Build the worker pool, returning the pool even if no workers were added.
    ///
    /// This is useful for dynamic worker addition later.
    pub fn build_allow_empty(self) -> WorkerPool {
        let metrics_collector = self.metrics_collector.unwrap_or_else(|| Arc::new(NoOpMetrics));

        let mut pool = WorkerPool::with_concurrency(
            &self.name,
            self.strategy,
            self.concurrency_limit,
            metrics_collector,
        );
        
        for worker in self.workers {
            pool.add_worker(worker);
        }

        pool
    }
}

/// Placeholder handler used during middleware chain building.
/// In practice, the WorkerPool replaces this with actual worker dispatch.
#[allow(dead_code)]
struct PlaceholderHandler;

#[async_trait::async_trait]
impl MessageHandler for PlaceholderHandler {
    async fn handle(&self, _message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        // This should never be called in production
        // The WorkerPool's dispatch method wraps workers with proper handlers
        Err(WorkerError::ProcessingFailed(
            "PlaceholderHandler should not be invoked".to_string()
        ))
    }
}

/// Wrapper to convert Box<dyn MessageHandler> to Arc-compatible type
#[allow(dead_code)]
struct ArcWrapper(Box<dyn MessageHandler>);

#[async_trait::async_trait]
impl MessageHandler for ArcWrapper {
    async fn handle(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        self.0.handle(message).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ReceivedMessage, AckHandle};
    use async_trait::async_trait;
    use std::time::Instant;

    #[derive(Debug)]
    #[allow(unused)]
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
    }

    impl TestWorker {
        fn new(id: &str) -> Self {
            Self {
                id: id.to_string(),
            }
        }
    }

    #[async_trait]
    impl Worker for TestWorker {
        fn id(&self) -> &str {
            &self.id
        }

        async fn process(&self, _message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
            Ok(())
        }
    }

    #[test]
    fn test_builder_creation() {
        let builder = WorkerPoolBuilder::new("test-pool");
        assert_eq!(builder.name, "test-pool");
    }

    #[test]
    fn test_builder_with_strategy() {
        let builder = WorkerPoolBuilder::new("test-pool")
            .with_strategy(LoadBalancingStrategy::Random);
        assert!(matches!(builder.strategy, LoadBalancingStrategy::Random));
    }

    #[test]
    fn test_builder_with_concurrency_limit() {
        let builder = WorkerPoolBuilder::new("test-pool")
            .with_concurrency_limit(50);
        assert_eq!(builder.concurrency_limit, 50);
    }

    #[test]
    fn test_builder_add_worker() {
        let builder = WorkerPoolBuilder::new("test-pool")
            .add_worker(TestWorker::new("worker-1"));
        assert_eq!(builder.workers.len(), 1);
    }

    #[test]
    fn test_builder_add_multiple_workers() {
        let workers = vec![
            TestWorker::new("worker-1"),
            TestWorker::new("worker-2"),
            TestWorker::new("worker-3"),
        ];

        let builder = WorkerPoolBuilder::new("test-pool")
            .add_workers(workers);
        assert_eq!(builder.workers.len(), 3);
    }

    #[test]
    fn test_builder_build_success() {
        let result = WorkerPoolBuilder::new("test-pool")
            .add_worker(TestWorker::new("worker-1"))
            .add_worker(TestWorker::new("worker-2"))
            .build();

        assert!(result.is_ok());
        let pool = result.unwrap();
        assert_eq!(pool.worker_count(), 2);
        assert_eq!(pool.name(), "test-pool");
    }

    #[test]
    fn test_builder_build_no_workers_error() {
        let result = WorkerPoolBuilder::new("test-pool").build();

        assert!(result.is_err());
        match result.unwrap_err() {
            WorkerError::ConfigError(msg) => {
                assert!(msg.contains("without any workers"));
            }
            _ => panic!("Expected ConfigError"),
        }
    }

    #[test]
    fn test_builder_build_allow_empty() {
        let pool = WorkerPoolBuilder::new("test-pool")
            .build_allow_empty();

        assert_eq!(pool.worker_count(), 0);
        assert_eq!(pool.name(), "test-pool");
    }

    #[test]
    fn test_builder_chaining() {
        let result = WorkerPoolBuilder::new("test-pool")
            .with_strategy(LoadBalancingStrategy::LeastLoaded)
            .with_concurrency_limit(100)
            .add_worker(TestWorker::new("worker-1"))
            .add_worker(TestWorker::new("worker-2"))
            .build();

        assert!(result.is_ok());
        let pool = result.unwrap();
        assert_eq!(pool.worker_count(), 2);
    }

    #[test]
    fn test_builder_with_metrics_collector() {
        struct MockMetrics;
        impl WorkerMetrics for MockMetrics {
            fn record_message_received(&self, _worker_id: &str, _queue_name: &str) {}
            fn record_message_processed(&self, _worker_id: &str, _queue_name: &str, _start_time: Instant) {}
            fn record_message_failed(&self, _worker_id: &str, _queue_name: &str, _error_type: &str, _start_time: Instant) {}
            fn record_message_retried(&self, _worker_id: &str, _queue_name: &str, _attempt: u32) {}
            fn record_message_retries_exhausted(&self, _worker_id: &str, _queue_name: &str) {}
            fn record_message_sent_to_dlq(&self, _queue_name: &str, _is_poison_pill: bool) {}
            fn record_active_workers(&self, _count: usize) {}
            fn record_in_flight_messages(&self, _count: usize) {}
        }

        let metrics_collector = Arc::new(MockMetrics);
        let pool = WorkerPoolBuilder::new("test-pool")
            .add_worker(TestWorker::new("worker-1"))
            .with_metrics_collector(metrics_collector.clone())
            .build()
            .unwrap();

        // Assert that the pool has the custom metrics collector (by comparing Arc pointers)
        // Note: metrics_collector is private, so we can't directly access it in tests.
        // We'll just verify the pool was built successfully.
        assert_eq!(pool.worker_count(), 1);
    }
}