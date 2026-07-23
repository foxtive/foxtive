//! Error types for the supervisor system

use thiserror::Error;

/// A type alias for results returned by supervisor operations.
pub type SupervisorResult<T> = Result<T, SupervisorError>;

/// Error types for the foxtive-supervisor crate.
#[derive(Error, Debug)]
pub enum SupervisorError {
    #[error("Configuration error for task '{task_id}': {reason}")]
    ConfigurationError { task_id: String, reason: String },

    #[error("Dependency validation failed for task '{task_id}': '{dependency_id}' - {error:?}")]
    DependencyValidation {
        task_id: String,
        dependency_id: String,
        error: ValidationError,
    },

    #[error("Circular dependency detected between '{task_a}' and '{task_b}'")]
    CircularDependency { task_a: String, task_b: String },

    #[error("Prerequisite '{name}' failed")]
    PrerequisiteFailed {
        name: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Task '{0}' not found")]
    UnknownTask(String),

    #[error("Persistence error: {message}")]
    PersistenceError {
        message: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Task panicked: {task_id}")]
    TaskPanicked {
        task_id: String,
        #[source]
        source: tokio::task::JoinError,
    },

    #[error("Channel error: {0}")]
    ChannelError(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Coordination error: {0}")]
    CoordinationError(String),

    #[error("{0}")]
    Internal(String),

    /// Wraps infrastructure errors (DB, Redis, IO, serialization, etc.)
    /// Carries source for error chaining.
    #[error("{message}")]
    Infrastructure {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    UnknownTaskId,
    CircularDependency,
}

impl SupervisorError {
    pub fn config(task_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::ConfigurationError {
            task_id: task_id.into(),
            reason: reason.into(),
        }
    }

    pub fn dependency_validation(
        task_id: &str,
        dependency_id: &str,
        error: ValidationError,
    ) -> Self {
        Self::DependencyValidation {
            task_id: task_id.to_string(),
            dependency_id: dependency_id.to_string(),
            error,
        }
    }

    pub fn circular_dependency(task_a: &str, task_b: &str) -> Self {
        Self::CircularDependency {
            task_a: task_a.to_string(),
            task_b: task_b.to_string(),
        }
    }

    pub fn prerequisite_failed(
        name: &str,
        source: Box<dyn std::error::Error + Send + Sync>,
    ) -> Self {
        Self::PrerequisiteFailed {
            name: name.to_string(),
            source,
        }
    }

    pub fn unknown_task(id: &str) -> Self {
        Self::UnknownTask(id.to_string())
    }

    pub fn persistence(message: impl Into<String>, source: std::io::Error) -> Self {
        Self::PersistenceError {
            message: message.into(),
            source,
        }
    }

    pub fn task_panicked(task_id: impl Into<String>, source: tokio::task::JoinError) -> Self {
        Self::TaskPanicked {
            task_id: task_id.into(),
            source,
        }
    }

    pub fn channel(message: impl Into<String>) -> Self {
        Self::ChannelError(message.into())
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::Timeout(message.into())
    }

    pub fn coordination(message: impl Into<String>) -> Self {
        Self::CoordinationError(message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    /// Wraps any error into an `Infrastructure` SupervisorError.
    ///
    /// This is the catch-all constructor for error types that don't have
    /// a dedicated `From` impl. Use with `.map_err()`:
    ///
    /// ```no_run
    /// use foxtive_supervisor::error::{SupervisorError, SupervisorResult};
    ///
    /// fn example() -> SupervisorResult<()> {
    ///     let flag = "true".parse::<bool>().map_err(SupervisorError::wrap)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn wrap(e: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Infrastructure {
            message: format!("{e}"),
            source: Some(Box::new(e)),
        }
    }
}

impl From<String> for SupervisorError {
    fn from(msg: String) -> Self {
        SupervisorError::Internal(msg)
    }
}

impl From<&str> for SupervisorError {
    fn from(msg: &str) -> Self {
        SupervisorError::Internal(msg.to_string())
    }
}

impl From<std::io::Error> for SupervisorError {
    fn from(err: std::io::Error) -> Self {
        SupervisorError::PersistenceError {
            message: err.to_string(),
            source: err,
        }
    }
}

impl From<tokio::task::JoinError> for SupervisorError {
    fn from(err: tokio::task::JoinError) -> Self {
        if err.is_panic() {
            SupervisorError::TaskPanicked {
                task_id: "unknown".to_string(),
                source: err,
            }
        } else {
            SupervisorError::ChannelError(format!("Task cancelled: {err}"))
        }
    }
}

#[cfg(feature = "distributed")]
impl From<redis::RedisError> for SupervisorError {
    fn from(err: redis::RedisError) -> Self {
        SupervisorError::CoordinationError(err.to_string())
    }
}
