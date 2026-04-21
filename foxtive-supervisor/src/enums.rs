use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SupervisionStatus {
    /// Task completed successfully (expected termination)
    CompletedNormally,
    /// Task failed and won't restart (max attempts reached)
    MaxAttemptsReached,
    /// Task was manually stopped
    ManuallyStopped,
    /// Restart was prevented by should_restart hook
    RestartPrevented,
    /// Setup failed, task never started
    SetupFailed,
    /// A declared dependency failed its setup phase
    DependencyFailed,
    /// Task was stopped because the circuit breaker opened
    CircuitBreakerOpened,
}

/// Defines when and how often a task should restart after failure
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum RestartPolicy {
    /// Always restart, no matter how many times it fails
    /// Use for: Critical services that must always run
    #[default]
    Always,

    /// Restart up to N times, then give up
    /// Use for: Tasks with a reasonable failure threshold
    MaxAttempts(usize),

    /// Never restart (task runs once)
    /// Use for: One-shot tasks, testing, or explicit manual control
    Never,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskState {
    /// Task is running normally
    Running,
    /// Task is paused (not processing)
    Paused,
    /// Task is in error state but will retry
    Retrying,
    /// Task is shutting down
    ShuttingDown,
    /// Task has stopped
    Stopped,
    /// Task's circuit breaker is open (no executions allowed)
    CircuitBreakerOpen,
}

/// Control messages sent to supervised tasks
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ControlMessage {
    /// Pause the task (if supported)
    Pause,
    /// Resume a paused task
    Resume,
    /// Stop the task permanently
    Stop,
    /// Force an immediate restart
    Restart,
    /// Reset the task's circuit breaker
    ResetCircuitBreaker,
}

/// Lifecycle events emitted by the supervisor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SupervisorEvent {
    /// A new task was registered
    TaskRegistered { id: String, name: String },
    /// A task started its setup phase
    TaskSetupStarted { id: String, name: String },
    /// A task completed its setup phase successfully
    TaskSetupCompleted { id: String, name: String },
    /// A task setup failed
    TaskSetupFailed {
        id: String,
        name: String,
        error: String,
    },
    /// A task's execution started (or restarted)
    TaskStarted {
        id: String,
        name: String,
        attempt: usize,
    },
    /// A task's execution finished successfully
    TaskFinished {
        id: String,
        name: String,
        attempt: usize,
    },
    /// A task's execution failed with an error
    TaskFailed {
        id: String,
        name: String,
        attempt: usize,
        error: String,
    },
    /// A task's execution panicked
    TaskPanicked {
        id: String,
        name: String,
        attempt: usize,
        panic_info: String,
    },
    /// A task is entering backoff before restart
    TaskBackoff {
        id: String,
        name: String,
        attempt: usize,
        delay: Duration,
    },
    /// A task was manually stopped
    TaskStopped { id: String, name: String },
    /// A task was paused
    TaskPaused { id: String, name: String },
    /// A task was resumed
    TaskResumed { id: String, name: String },
    /// A task reached its maximum restart attempts
    TaskMaxAttemptsReached {
        id: String,
        name: String,
        attempts: usize,
    },
    /// A task restart was prevented by a hook
    TaskRestartPrevented {
        id: String,
        name: String,
        attempt: usize,
    },
    /// A task was removed from the supervisor
    TaskRemoved { id: String, name: String },
    /// A task's circuit breaker tripped (opened)
    CircuitBreakerTripped {
        id: String,
        name: String,
        consecutive_failures: usize,
    },
    /// A task's circuit breaker was reset (closed)
    CircuitBreakerReset { id: String, name: String },
    /// A task's circuit breaker entered half-open state
    CircuitBreakerHalfOpen { id: String, name: String },
    /// Supervisor is shutting down
    SupervisorShutdownStarted,
    /// Supervisor completed shutdown
    SupervisorShutdownCompleted,
    /// A task's configuration was updated at runtime
    TaskConfigUpdated {
        id: String,
        name: String,
        field: String,
        old_value: String,
        new_value: String,
    },
}

/// Health status for monitoring and observability
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Task is operating normally
    Healthy,

    /// Task is functioning but with reduced capacity or warnings
    Degraded { reason: String },

    /// Task is not functioning properly
    Unhealthy { reason: String },

    /// Task is in unknown state (initialization, transition)
    #[default]
    Unknown,
}

/// Configuration for a task's circuit breaker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before the circuit opens
    pub failure_threshold: usize,
    /// How long the circuit stays open before transitioning to half-open
    pub reset_timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(60),
        }
    }
}

/// Runtime configuration for a supervised task that can be hot-reloaded
#[derive(Debug, Clone)]
pub struct TaskConfig {
    /// Restart policy for the task
    pub restart_policy: RestartPolicy,
    /// Backoff strategy between restart attempts
    pub backoff_strategy: BackoffStrategy,
    /// Whether the task is enabled (can be disabled without removing)
    pub enabled: bool,
}

impl TaskConfig {
    /// Create a new TaskConfig from a SupervisedTask's current configuration
    pub fn from_task(task: &dyn crate::contracts::SupervisedTask) -> Self {
        Self {
            restart_policy: task.restart_policy(),
            backoff_strategy: task.backoff_strategy(),
            enabled: true,
        }
    }

    /// Create a TaskConfig with default values
    pub fn new() -> Self {
        Self {
            restart_policy: RestartPolicy::default(),
            backoff_strategy: BackoffStrategy::default(),
            enabled: true,
        }
    }
}

impl Default for TaskConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Defines the delay between restart attempts
pub enum BackoffStrategy {
    /// Fixed delay between restarts
    ///
    /// Example: Always wait 5 seconds
    Fixed(Duration),

    /// Exponential backoff: initial * 2^(attempt-1), capped at max
    ///
    /// Example: 2s -> 4s -> 8s -> 16s -> ... -> 60s (max)
    ///
    /// Best for: Network failures, external service issues
    Exponential { initial: Duration, max: Duration },

    /// Linear backoff: initial + (increment * attempt)
    ///
    /// Example: 5s -> 10s -> 15s -> 20s -> ... -> 60s (max)
    ///
    /// Best for: Predictable delay patterns
    Linear {
        initial: Duration,
        increment: Duration,
        max: Duration,
    },

    /// Fibonacci backoff: delays follow Fibonacci sequence
    ///
    /// Example: 1s -> 1s -> 2s -> 3s -> 5s -> 8s -> ... -> max
    ///
    /// Best for: Graceful degradation scenarios
    Fibonacci { initial: Duration, max: Duration },

    /// Custom backoff with user-defined delay calculation
    ///
    /// Receives attempt number, returns delay duration
    Custom(Box<dyn Fn(usize) -> Duration + Send + Sync>),
}

// Manual Debug implementation
impl std::fmt::Debug for BackoffStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fixed(duration) => f.debug_tuple("Fixed").field(duration).finish(),
            Self::Exponential { initial, max } => f
                .debug_struct("Exponential")
                .field("initial", initial)
                .field("max", max)
                .finish(),
            Self::Linear {
                initial,
                increment,
                max,
            } => f
                .debug_struct("Linear")
                .field("initial", initial)
                .field("increment", increment)
                .field("max", max)
                .finish(),
            Self::Fibonacci { initial, max } => f
                .debug_struct("Fibonacci")
                .field("initial", initial)
                .field("max", max)
                .finish(),
            Self::Custom(_) => f.debug_tuple("Custom").field(&"<function>").finish(),
        }
    }
}

// Manual Clone implementation
impl Clone for BackoffStrategy {
    fn clone(&self) -> Self {
        match self {
            Self::Fixed(duration) => Self::Fixed(*duration),
            Self::Exponential { initial, max } => Self::Exponential {
                initial: *initial,
                max: *max,
            },
            Self::Linear {
                initial,
                increment,
                max,
            } => Self::Linear {
                initial: *initial,
                increment: *increment,
                max: *max,
            },
            Self::Fibonacci { initial, max } => Self::Fibonacci {
                initial: *initial,
                max: *max,
            },
            Self::Custom(_) => {
                // For Custom variant, we clone with default exponential strategy
                // This is a limitation - custom functions can't be cloned
                Self::default()
            }
        }
    }
}

impl Default for BackoffStrategy {
    fn default() -> Self {
        Self::Exponential {
            initial: Duration::from_secs(2),
            max: Duration::from_secs(60),
        }
    }
}

impl BackoffStrategy {
    /// Calculate the delay for a given attempt number (1-indexed)
    pub fn calculate_delay(&self, attempt: usize) -> Duration {
        match self {
            Self::Fixed(duration) => *duration,

            Self::Exponential { initial, max } => {
                // 2^(attempt-1), but cap the exponent to prevent overflow
                let exponent = attempt.saturating_sub(1).min(20);
                let multiplier = 2_u32.saturating_pow(exponent as u32);
                initial.saturating_mul(multiplier).min(*max)
            }

            Self::Linear {
                initial,
                increment,
                max,
            } => {
                let total_increment = increment.saturating_mul(attempt.saturating_sub(1) as u32);
                initial.saturating_add(total_increment).min(*max)
            }

            Self::Fibonacci { initial, max } => {
                let fib_multiplier = Self::fibonacci(attempt);
                initial.saturating_mul(fib_multiplier as u32).min(*max)
            }

            Self::Custom(func) => func(attempt),
        }
    }

    /// Calculate Fibonacci number for backoff
    fn fibonacci(n: usize) -> usize {
        match n {
            0 => 0,
            1 | 2 => 1,
            n => {
                let mut a: usize = 1;
                let mut b = 1;
                for _ in 2..n {
                    let next = a.saturating_add(b);
                    a = b;
                    b = next;
                }
                b
            }
        }
    }

    /// Create a fixed delay strategy
    pub fn fixed(duration: Duration) -> Self {
        Self::Fixed(duration)
    }

    /// Create exponential backoff with defaults (2s -> 60s)
    pub fn exponential() -> Self {
        Self::Exponential {
            initial: Duration::from_secs(2),
            max: Duration::from_secs(60),
        }
    }

    /// Create exponential backoff with custom parameters
    pub fn exponential_custom(initial: Duration, max: Duration) -> Self {
        Self::Exponential { initial, max }
    }

    /// Create linear backoff with defaults (5s increments, max 60s)
    pub fn linear() -> Self {
        Self::Linear {
            initial: Duration::from_secs(5),
            increment: Duration::from_secs(5),
            max: Duration::from_secs(60),
        }
    }

    /// Create linear backoff with custom parameters
    pub fn linear_custom(initial: Duration, increment: Duration, max: Duration) -> Self {
        Self::Linear {
            initial,
            increment,
            max,
        }
    }

    /// Create Fibonacci backoff with defaults (1s -> 60s)
    pub fn fibonacci_with_default() -> Self {
        Self::Fibonacci {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(60),
        }
    }

    /// Create custom backoff with user-defined function
    pub fn custom<F>(func: F) -> Self
    where
        F: Fn(usize) -> Duration + Send + Sync + 'static,
    {
        Self::Custom(Box::new(func))
    }
}

impl HealthStatus {
    /// Check if the status is healthy
    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy)
    }

    /// Check if the status is unhealthy
    pub fn is_unhealthy(&self) -> bool {
        matches!(self, Self::Unhealthy { .. })
    }

    /// Check if the status is degraded
    pub fn is_degraded(&self) -> bool {
        matches!(self, Self::Degraded { .. })
    }

    /// Get the reason for non-healthy status
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Degraded { reason } | Self::Unhealthy { reason } => Some(reason),
            _ => None,
        }
    }

    /// Create a healthy status
    pub fn healthy() -> Self {
        Self::Healthy
    }

    /// Create a degraded status with reason
    pub fn degraded(reason: impl Into<String>) -> Self {
        Self::Degraded {
            reason: reason.into(),
        }
    }

    /// Create an unhealthy status with reason
    pub fn unhealthy(reason: impl Into<String>) -> Self {
        Self::Unhealthy {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_backoff() {
        let strategy = BackoffStrategy::exponential();

        assert_eq!(strategy.calculate_delay(1), Duration::from_secs(2));
        assert_eq!(strategy.calculate_delay(2), Duration::from_secs(4));
        assert_eq!(strategy.calculate_delay(3), Duration::from_secs(8));
        assert_eq!(strategy.calculate_delay(4), Duration::from_secs(16));
        assert_eq!(strategy.calculate_delay(5), Duration::from_secs(32));
        assert_eq!(strategy.calculate_delay(6), Duration::from_secs(60)); // capped at max
    }

    #[test]
    fn test_linear_backoff() {
        let strategy = BackoffStrategy::linear();

        assert_eq!(strategy.calculate_delay(1), Duration::from_secs(5));
        assert_eq!(strategy.calculate_delay(2), Duration::from_secs(10));
        assert_eq!(strategy.calculate_delay(3), Duration::from_secs(15));
        assert_eq!(strategy.calculate_delay(4), Duration::from_secs(20));
    }

    #[test]
    fn test_fibonacci_backoff() {
        let strategy = BackoffStrategy::fibonacci_with_default();

        assert_eq!(strategy.calculate_delay(1), Duration::from_secs(1));
        assert_eq!(strategy.calculate_delay(2), Duration::from_secs(1));
        assert_eq!(strategy.calculate_delay(3), Duration::from_secs(2));
        assert_eq!(strategy.calculate_delay(4), Duration::from_secs(3));
        assert_eq!(strategy.calculate_delay(5), Duration::from_secs(5));
        assert_eq!(strategy.calculate_delay(6), Duration::from_secs(8));
    }

    #[test]
    fn test_fixed_backoff() {
        let strategy = BackoffStrategy::fixed(Duration::from_secs(10));

        assert_eq!(strategy.calculate_delay(1), Duration::from_secs(10));
        assert_eq!(strategy.calculate_delay(5), Duration::from_secs(10));
        assert_eq!(strategy.calculate_delay(100), Duration::from_secs(10));
    }

    #[test]
    fn test_custom_backoff() {
        let strategy = BackoffStrategy::custom(|attempt| Duration::from_secs(attempt as u64 * 3));

        assert_eq!(strategy.calculate_delay(1), Duration::from_secs(3));
        assert_eq!(strategy.calculate_delay(2), Duration::from_secs(6));
        assert_eq!(strategy.calculate_delay(5), Duration::from_secs(15));
    }

    #[test]
    fn test_health_status() {
        let healthy = HealthStatus::healthy();
        assert!(healthy.is_healthy());
        assert!(!healthy.is_unhealthy());
        assert_eq!(healthy.reason(), None);

        let degraded = HealthStatus::degraded("High memory usage");
        assert!(degraded.is_degraded());
        assert_eq!(degraded.reason(), Some("High memory usage"));

        let unhealthy = HealthStatus::unhealthy("Connection lost");
        assert!(unhealthy.is_unhealthy());
        assert_eq!(unhealthy.reason(), Some("Connection lost"));
    }

    #[test]
    fn test_backoff_debug() {
        let exponential = BackoffStrategy::exponential();
        let debug_str = format!("{:?}", exponential);
        assert!(debug_str.contains("Exponential"));

        let custom = BackoffStrategy::custom(|n| Duration::from_secs(n as u64));
        let debug_str = format!("{:?}", custom);
        assert!(debug_str.contains("Custom"));
    }

    #[test]
    fn test_backoff_clone() {
        let original = BackoffStrategy::exponential();
        let cloned = original.clone();

        assert_eq!(original.calculate_delay(3), cloned.calculate_delay(3));
    }
}
