use crate::enums::{
    BackoffStrategy, CircuitBreakerConfig, HealthStatus, RestartPolicy, SupervisorEvent, TaskState,
};
use std::time::Duration;

/// Core trait for any long-running task that needs supervision
#[async_trait::async_trait]
pub trait SupervisedTask: Send + Sync {
    // REQUIRED METHODS

    /// Unique identifier for this task (used in logs, monitoring, and dependency resolution)
    fn id(&self) -> &'static str;

    /// Human-readable name for this task (used in logs and monitoring)
    fn name(&self) -> String {
        self.id().to_string()
    }

    /// Main task execution - this should run until completion or error
    async fn run(&self) -> anyhow::Result<()>;

    // DEPENDENCY MANAGEMENT

    /// Declare task IDs that must complete setup before this task starts
    fn dependencies(&self) -> &'static [&'static str] {
        &[]
    }

    // OPTIONAL CONFIGURATION

    /// Restart policy when task fails or panics
    fn restart_policy(&self) -> RestartPolicy {
        RestartPolicy::Always
    }

    /// Backoff strategy between restart attempts
    fn backoff_strategy(&self) -> BackoffStrategy {
        BackoffStrategy::default()
    }

    /// Priority of the task (higher values = higher priority)
    /// Used when concurrency limits are reached to decide which tasks start first
    fn priority(&self) -> i32 {
        0
    }

    /// Optional concurrency limit for this specific task
    /// This is useful if the task is part of a pool or if multiple instances exist
    fn concurrency_limit(&self) -> Option<usize> {
        None
    }

    /// Configuration for the task's circuit breaker
    /// If `None`, the circuit breaker is disabled for this task
    fn circuit_breaker(&self) -> Option<CircuitBreakerConfig> {
        None
    }

    /// Maximum time to wait for the task to finish its `run()` and `on_shutdown()`
    /// before forced termination during supervisor shutdown.
    fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(30)
    }

    /// Optional cron expression for scheduled execution
    #[cfg(feature = "cron")]
    fn cron_schedule(&self) -> Option<&'static str> {
        None
    }

    /// Optional initial delay before the first execution
    ///
    /// This is useful for staggering task startup to prevent thundering herd problems
    /// or for giving dependencies time to initialize.
    fn initial_delay(&self) -> Option<Duration> {
        None
    }

    /// Optional jitter to add randomness to the initial delay
    ///
    /// Returns a tuple of (min_jitter, max_jitter) that will be randomly added
    /// to the initial delay. This helps prevent thundering herd problems in
    /// distributed deployments where multiple instances start simultaneously.
    ///
    /// Example: If initial_delay is 5s and jitter is (0, 2s), the actual delay
    /// will be between 5s and 7s.
    fn jitter(&self) -> Option<(Duration, Duration)> {
        None
    }

    /// Minimum time between restart attempts (rate limiting)
    ///
    /// This prevents tasks from restarting too frequently, which can overwhelm
    /// external services or resources. If a task fails and wants to restart,
    /// it will wait at least this duration before attempting again.
    ///
    /// This is applied in addition to the backoff strategy - the actual delay
    /// will be the maximum of the backoff delay and this minimum restart interval.
    fn min_restart_interval(&self) -> Option<Duration> {
        None
    }

    /// Time window restrictions for task execution
    ///
    /// Returns optional start and end times (as hours in 24-hour format) during
    /// which the task is allowed to run. Outside this window, the task will wait
    /// until the next allowed time.
    ///
    /// Example: Some((9, 17)) means the task can only run between 9 AM and 5 PM.
    /// Use None for either value to indicate no restriction on that boundary.
    ///
    /// Note: Times are evaluated in UTC by default.
    fn execution_time_window(&self) -> Option<(Option<u8>, Option<u8>)> {
        None
    }

    /// Task group identifier for atomic operations
    ///
    /// Tasks with the same group ID can be started, stopped, or managed together.
    /// This is useful for related tasks that should be treated as a unit.
    ///
    /// Example: A database connection pool and its query processor might be in
    /// the same group so they start and stop together.
    fn group_id(&self) -> Option<&'static str> {
        None
    }

    /// Conditional dependencies based on environment or runtime conditions
    ///
    /// Returns a list of dependencies that are only active when certain conditions are met.
    /// Each tuple contains (dependency_id, condition_function).
    /// The condition function receives the current environment context and returns true
    /// if the dependency should be enforced.
    ///
    /// Example: Only depend on "cache-service" if USE_CACHE env var is set
    /// ```ignore
    /// vec![("cache-service", Box::new(|| std::env::var("USE_CACHE").is_ok()))]
    /// ```
    #[allow(clippy::type_complexity)]
    fn conditional_dependencies(&self) -> Vec<(&'static str, Box<dyn Fn() -> bool + Send + Sync>)> {
        Vec::new()
    }

    /// Get all active dependencies (regular + conditional that evaluate to true)
    ///
    /// This combines regular dependencies with conditional dependencies whose
    /// conditions currently evaluate to true.
    fn active_dependencies(&self) -> Vec<&'static str> {
        let mut deps = self.dependencies().to_vec();

        // Add conditional dependencies whose conditions are met
        for (dep_id, condition) in self.conditional_dependencies() {
            if condition() {
                deps.push(dep_id);
            }
        }

        deps
    }

    // LIFECYCLE HOOKS

    /// Called once before the first run() attempt
    async fn setup(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Called after task stops (success, failure, or panic)
    ///
    /// **Purpose:** Internal resource cleanup and teardown.
    ///
    /// This hook is called automatically by the supervision loop whenever a task's
    /// `run()` method completes (successfully or with error) or panics. It's meant for:
    /// - Closing database connections
    /// - Releasing file handles
    /// - Dropping temporary resources
    /// - Any internal cleanup needed before restart or shutdown
    ///
    /// **When it's called:**
    /// - After every `run()` completion (success or failure)
    /// - After a panic is caught
    /// - When task receives Stop command
    /// - During supervisor shutdown
    ///
    /// **Important:** This is NOT for graceful shutdown logic. Use `on_shutdown()` instead
    /// if you need to perform actions specifically during supervisor-initiated shutdown.
    async fn cleanup(&self) {
        // Default: no cleanup
    }

    // RESTART CONTROL

    /// Dynamic restart control - called before each restart
    async fn should_restart(&self, _attempt: usize, _last_error: &str) -> bool {
        true
    }

    // MONITORING & OBSERVABILITY

    /// Health check for monitoring endpoints
    async fn health_check(&self) -> HealthStatus {
        HealthStatus::Healthy
    }

    /// Return task-specific metrics (optional)
    async fn metrics(&self) -> Option<TaskMetrics> {
        None
    }

    /// Called before each restart (not called on first attempt)
    async fn on_restart(&self, _attempt: usize) {
        // Default: no action
    }

    // ERROR HANDLING HOOKS

    /// Called when run() returns an error
    async fn on_error(&self, _error: &str, _attempt: usize) {
        // Default: no action (supervisor logs by default)
    }

    /// Called when run() panics
    async fn on_panic(&self, _panic_info: &str, _attempt: usize) {
        // Default: no action (supervisor logs by default)
    }

    /// Called when the service is shutting down
    ///
    /// **Purpose:** Graceful shutdown hook for external/user-defined cleanup.
    ///
    /// This hook is called ONLY during supervisor-initiated graceful shutdown,
    /// AFTER the supervision loop has terminated. It's meant for:
    /// - Flushing buffers to disk/network
    /// - Sending final notifications
    /// - Performing graceful disconnections
    /// - Any user-defined shutdown logic that should run once at the end
    ///
    /// **When it's called:**
    /// - ONLY during `TaskRuntime::shutdown()`
    /// - AFTER the supervision loop ends
    /// - AFTER `cleanup()` has been called
    /// - Once per task, in dependency-aware order
    ///
    /// **Key difference from `cleanup()`:**
    /// - `cleanup()` = Internal teardown, called after EVERY run() completion
    /// - `on_shutdown()` = Graceful shutdown, called ONCE during supervisor shutdown
    ///
    /// **Example:**
    /// ```ignore
    /// async fn on_shutdown(&self) {
    ///     // Flush pending messages
    ///     self.message_queue.flush().await.ok();
    ///     // Notify monitoring service
    ///     self.notify_shutdown().await.ok();
    /// }
    /// ```
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

/// Trait for components that listen to supervisor events
#[async_trait::async_trait]
pub trait SupervisorEventListener: Send + Sync {
    /// Called when a supervisor event occurs
    async fn on_event(&self, event: SupervisorEvent);
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
