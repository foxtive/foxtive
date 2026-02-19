use crate::enums::{BackoffStrategy, HealthStatus, RestartPolicy, TaskState};

/// Core trait for any long-running task that needs supervision
///
/// This can be implemented for:
/// - RabbitMQ/Kafka consumers
/// - HTTP/WebSocket servers
/// - Cron jobs
/// - Background workers
/// - Database connection pools
/// - File watchers
/// - Any async task that should run continuously
#[async_trait::async_trait]
pub trait SupervisedTask: Send + Sync {
    // REQUIRED METHODS

    /// Unique identifier for this task (used in logs, monitoring, and dependency resolution)
    ///
    /// Must be unique across all registered tasks in the same supervisor.
    /// Defaults to the task name if not overridden.
    fn id(&self) -> &'static str;

    /// Human-readable name for this task (used in logs and monitoring)
    ///
    /// Defaults to task_id if not overridden.
    fn name(&self) -> String {
        self.id().to_string()
    }

    /// Main task execution - this should run until completion or error
    ///
    /// For long-running tasks (like consumers), this should block forever
    /// and only return on error or explicit shutdown.
    ///
    /// For one-shot tasks, return Ok(()) when complete.
    async fn run(&self) -> anyhow::Result<()>;

    // DEPENDENCY MANAGEMENT

    /// Declare task IDs that must complete setup before this task starts
    ///
    /// The supervisor will wait for all listed dependencies to successfully
    /// complete their `setup()` phase before calling `setup()` on this task.
    ///
    /// # Important
    /// - Dependencies are resolved by `task_id`, not by name
    /// - Circular dependencies will cause a startup error
    /// - If a dependency's setup fails, this task will not start
    ///
    /// # Example
    /// ```ignore
    /// fn dependencies(&self) -> &'static [&'static str] {
    ///     &["database", "redis"]
    /// }
    /// ```
    fn dependencies(&self) -> &'static [&'static str] {
        &[]
    }

    // OPTIONAL CONFIGURATION

    /// Restart policy when task fails or panics
    ///
    /// Default: Always restart
    fn restart_policy(&self) -> RestartPolicy {
        RestartPolicy::Always
    }

    /// Backoff strategy between restart attempts
    ///
    /// Default: Exponential backoff (2s -> 4s -> 8s -> ... max 60s)
    fn backoff_strategy(&self) -> BackoffStrategy {
        BackoffStrategy::default()
    }

    // LIFECYCLE HOOKS

    /// Called once before the first run() attempt
    ///
    /// Use for:
    /// - Initializing connections
    /// - Declaring queue topology
    /// - Loading configuration
    /// - Validating prerequisites
    ///
    /// If this fails, the task will not start
    async fn setup(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Called after task stops (success, failure, or panic)
    ///
    /// Guaranteed to run even if task panics.
    ///
    /// Use for:
    /// - Closing connections
    /// - Flushing buffers
    /// - Releasing resources
    /// - Final cleanup
    async fn cleanup(&self) {
        // Default: no cleanup
    }

    // RESTART CONTROL

    /// Dynamic restart control - called before each restart
    ///
    /// Return false to prevent restart (overrides restart_policy)
    ///
    /// Use for:
    /// - Preventing restart on specific error types
    /// - Circuit breaker pattern
    /// - Rate limiting restarts
    /// - Configuration-based shutdown
    ///
    /// # Arguments
    /// * `attempt` - The attempt number (1-indexed)
    /// * `last_error` - String representation of the last error/panic
    async fn should_restart(&self, _attempt: usize, _last_error: &str) -> bool {
        true
    }

    // MONITORING & OBSERVABILITY

    /// Health check for monitoring endpoints
    ///
    /// Use for:
    /// - Kubernetes liveness/readiness probes
    /// - Load balancer health checks
    /// - Monitoring dashboards
    /// - Alerting systems
    async fn health_check(&self) -> HealthStatus {
        HealthStatus::Healthy
    }

    /// Return task-specific metrics (optional)
    ///
    /// Use for:
    /// - Prometheus metrics
    /// - Custom monitoring
    /// - Performance tracking
    async fn metrics(&self) -> Option<TaskMetrics> {
        None
    }

    /// Called before each restart (not called on first attempt)
    ///
    /// Use for:
    /// - Resetting state
    /// - Re-establishing connections
    /// - Logging restart events
    async fn on_restart(&self, _attempt: usize) {
        // Default: no action
    }

    // ERROR HANDLING HOOKS

    /// Called when run() returns an error
    ///
    /// Use for:
    /// - Logging errors
    /// - Sending alerts
    /// - Recording metrics
    /// - Custom error handling
    async fn on_error(&self, _error: &str, _attempt: usize) {
        // Default: no action (supervisor logs by default)
    }

    /// Called when run() panics
    ///
    /// Use for:
    /// - Panic-specific alerting
    /// - Critical error handling
    /// - Dumping debug state
    async fn on_panic(&self, _panic_info: &str, _attempt: usize) {
        // Default: no action (supervisor logs by default)
    }

    /// Called when the service is shutting down
    ///
    /// Use for:
    /// - Cleanup tasks
    /// - Releasing resources
    /// - Graceful shutdown procedures
    async fn on_shutdown(&self) {
        // Default implementation does nothing
    }
}

/// Extension trait for tasks that need state tracking
#[async_trait::async_trait]
pub trait StatefulTask: SupervisedTask {
    /// Get the current state of the task
    async fn state(&self) -> TaskState;
}

/// Optional metrics that tasks can expose
#[derive(Debug, Clone)]
pub struct TaskMetrics {
    /// Number of successful operations
    pub success_count: u64,
    /// Number of failed operations
    pub error_count: u64,
    /// Number of messages/items processed
    pub processed_count: u64,
    /// Current processing rate (per second)
    pub processing_rate: f64,
    /// Average processing time (milliseconds)
    pub avg_processing_time_ms: f64,
    /// Custom metrics (key-value pairs)
    pub custom: std::collections::HashMap<String, f64>,
}

impl Default for TaskMetrics {
    fn default() -> Self {
        Self {
            success_count: 0,
            error_count: 0,
            processed_count: 0,
            processing_rate: 0.0,
            avg_processing_time_ms: 0.0,
            custom: std::collections::HashMap::new(),
        }
    }
}

impl TaskMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a custom metric
    pub fn add_custom(&mut self, key: impl Into<String>, value: f64) {
        self.custom.insert(key.into(), value);
    }
}