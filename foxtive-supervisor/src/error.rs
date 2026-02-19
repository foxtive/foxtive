//! Error types for the Foxtive supervisor
//!
//! This module defines comprehensive error types that provide detailed
//! information about various failure scenarios in the supervisor system.
//!
//! ## Enhanced Error Handling with thiserror
//!
//! The supervisor now provides enhanced error handling through `thiserror` derive macros,
//! making errors more structured and easier to work with. The `SupervisorError` enum
//! automatically implements `std::error::Error`, `Display`, and `Debug` traits.
//!
//! ## Error Propagation Examples
//!
//! ```rust
//! use foxtive_supervisor::{Supervisor, SupervisorError};
//! use foxtive_supervisor::error::{SupervisorResultExt, SupervisorErrorExt};
//! use anyhow::Context;
//! use std::io;
//!
//! // Easy conversion from standard errors to SupervisorError
//! fn file_operation() -> Result<(), io::Error> {
//!     // Some file operation that might fail
//!     Err(io::Error::new(io::ErrorKind::NotFound, "File not found"))
//! }
//!
//! fn example_usage() -> Result<(), SupervisorError> {
//!     // Convert standard errors to supervisor errors
//!     file_operation()
//!         .supervisor_context("File setup")?;
//!     
//!     Ok(())
//! }
//!
//! // Pattern matching with enhanced error information
//! fn handle_error(error: SupervisorError) {
//!     if error.is_configuration_error() {
//!         eprintln!("Configuration error: {}", error);
//!     } else if error.is_execution_error() {
//!         eprintln!("Execution error: {}", error);
//!         if let Some(source) = error.source_error() {
//!             eprintln!("Caused by: {}", source);
//!         }
//!     } else {
//!         eprintln!("Other error: {}", error);
//!     }
//! }
//! ```
//!
//! ## Error Kind Classification
//!
//! Errors are classified into four main kinds for easier handling:
//! - `ErrorKind::Configuration` - Setup and dependency issues
//! - `ErrorKind::Runtime` - Runtime and setup failures
//! - `ErrorKind::Execution` - Task execution problems
//! - `ErrorKind::System` - Internal system errors
//!
//! ## Error Handling Patterns
//!
//! The supervisor provides structured error handling through the `SupervisorError` enum.
//! Each variant contains contextual information to help developers understand and
//! handle failures appropriately.
//!
//! ## Example: Handling Dependency Validation Errors
//!
//! ```rust
//! use foxtive_supervisor::{Supervisor, SupervisorError};
//!
//! fn example_usage() -> Result<(), SupervisorError> {
//!     let mut supervisor = Supervisor::new();
//!     
//!     // This might fail with DependencyValidation error
//!     // supervisor.start()?; // Would require async context
//!     
//!     Ok(())
//! }
//!
//! // Pattern matching for specific error handling
//! fn handle_supervisor_error(error: SupervisorError) {
//!     match error {
//!         SupervisorError::DependencyValidation { task_id, dependency_id, reason } => {
//!             eprintln!("Task '{}' has invalid dependency '{}': {:?}", task_id, dependency_id, reason);
//!         }
//!         SupervisorError::CircularDependency { task_id, dependency_id } => {
//!             eprintln!("Circular dependency detected: '{}' -> '{}'", task_id, dependency_id);
//!         }
//!         e => {
//!             eprintln!("Supervisor failed: {}", e);
//!         }
//!     }
//! }
//! ```
//!
//! ## Error Categories
//!
//! ### Configuration Errors
//! - `DependencyValidation`: Invalid task dependencies
//! - `CircularDependency`: Circular dependency in task graph
//! - `InvalidConfiguration`: General configuration issues
//!
//! ### Runtime Errors
//! - `PrerequisiteFailed`: Prerequisite tasks failed to complete
//! - `SetupFailed`: Task setup phase failed
//! - `DependencySetupFailed`: A dependency's setup failed
//!
//! ### Execution Errors
//! - `TaskExecutionFailed`: Task failed during execution
//! - `TaskPanicked`: Task panicked during execution
//! - `MaxAttemptsReached`: Maximum restart attempts exceeded
//! - `RestartPrevented`: Custom restart prevention logic triggered
//!
//! ### System Errors
//! - `RuntimeFailure`: Generic runtime failures
//! - `InternalError`: Unexpected internal supervisor errors

use thiserror::Error;

/// Extension trait for Result types to provide supervisor-specific error handling
pub trait SupervisorResultExt<T> {
    /// Convert any error to a SupervisorError
    fn supervisor_error(self) -> Result<T, SupervisorError>;
    
    /// Add supervisor context to the error
    fn supervisor_context<C>(self, context: C) -> Result<T, SupervisorError>
    where
        C: std::fmt::Display + Send + Sync + 'static;
}

impl<T, E> SupervisorResultExt<T> for Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn supervisor_error(self) -> Result<T, SupervisorError> {
        self.map_err(|e| SupervisorError::RuntimeFailure {
            operation: "unknown".to_string(),
            source: anyhow::Error::from(e),
        })
    }
    
    fn supervisor_context<C>(self, context: C) -> Result<T, SupervisorError>
    where
        C: std::fmt::Display + Send + Sync + 'static,
    {
        self.map_err(|e| SupervisorError::RuntimeFailure {
            operation: context.to_string(),
            source: anyhow::Error::from(e),
        })
    }
}

/// Extension trait for SupervisorError to provide additional utilities
pub trait SupervisorErrorExt {
    /// Check if this error matches a specific validation error
    fn is_validation_error(&self, validation_type: &ValidationError) -> bool;
    
    /// Get the underlying source error if it exists
    fn source_error(&self) -> Option<&(dyn std::error::Error + 'static)>;
}

impl SupervisorErrorExt for SupervisorError {
    fn is_validation_error(&self, validation_type: &ValidationError) -> bool {
        match self {
            SupervisorError::DependencyValidation { reason, .. } => reason == validation_type,
            _ => false,
        }
    }
    
    fn source_error(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(self)
    }
}

/// Error kinds for categorizing supervisor errors
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorKind {
    /// Configuration-related errors
    Configuration,
    /// Runtime/setup related errors
    Runtime,
    /// Task execution errors
    Execution,
    /// System/internal errors
    System,
}

/// Specific validation error types
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ValidationError {
    /// Unknown task ID referenced in dependencies
    #[error("unknown task ID")]
    UnknownTaskId,
    /// Circular dependency detected
    #[error("circular dependency")]
    CircularDependency,
    /// Invalid task configuration
    #[error("invalid configuration")]
    InvalidConfiguration,
}

/// Comprehensive error enum for supervisor operations
#[derive(Error, Debug)]
pub enum SupervisorError {
    /// Task dependency validation failed
    #[error("Task '{task_id}' has invalid dependency '{dependency_id}': {reason}")]
    DependencyValidation {
        /// The task that declared the invalid dependency
        task_id: String,
        /// The invalid dependency ID
        dependency_id: String,
        /// Reason for the validation failure
        reason: ValidationError,
    },

    /// Circular dependency detected in task graph
    #[error("Circular dependency detected: '{task_id}' -> '{dependency_id}'")]
    CircularDependency {
        /// The task that creates the cycle
        task_id: String,
        /// The dependency that causes the cycle
        dependency_id: String,
    },

    /// Prerequisite failed to complete successfully
    #[error("Prerequisite '{name}' failed to complete")]
    PrerequisiteFailed {
        /// Name of the prerequisite
        name: String,
        /// Underlying error cause
        #[source]
        source: anyhow::Error,
    },

    /// Task setup phase failed
    #[error("Task '{task_name}' setup failed")]
    SetupFailed {
        /// Task identifier
        task_id: String,
        /// Task name for logging
        task_name: String,
        /// Underlying error cause
        #[source]
        source: anyhow::Error,
    },

    /// Task dependency setup failed
    #[error("Task '{task_name}' dependency '{dependency_id}' setup failed: {error_message}")]
    DependencySetupFailed {
        /// The dependent task
        task_id: String,
        /// The task name for logging
        task_name: String,
        /// The failed dependency
        dependency_id: String,
        /// Error message from the dependency
        error_message: String,
    },

    /// Task execution failed
    #[error("Task '{task_name}' failed on attempt #{attempt}")]
    TaskExecutionFailed {
        /// Task identifier
        task_id: String,
        /// Task name for logging
        task_name: String,
        /// Current attempt number
        attempt: usize,
        /// Underlying error cause
        #[source]
        source: anyhow::Error,
    },

    /// Task panicked during execution
    #[error("Task '{task_name}' panicked on attempt #{attempt}: {panic_message}")]
    TaskPanicked {
        /// Task identifier
        task_id: String,
        /// Task name for logging
        task_name: String,
        /// Current attempt number
        attempt: usize,
        /// Panic message
        panic_message: String,
    },

    /// Maximum restart attempts reached
    #[error("Task '{task_name}' reached maximum attempts ({actual_attempts}/{max_attempts})")]
    MaxAttemptsReached {
        /// Task identifier
        task_id: String,
        /// Task name for logging
        task_name: String,
        /// Maximum allowed attempts
        max_attempts: usize,
        /// Actual attempts made
        actual_attempts: usize,
    },

    /// Task restart was prevented by custom logic
    #[error("Task '{task_name}' restart prevented on attempt #{attempt}: {reason}")]
    RestartPrevented {
        /// Task identifier
        task_id: String,
        /// Task name for logging
        task_name: String,
        /// Current attempt number
        attempt: usize,
        /// Reason for prevention
        reason: String,
    },

    /// Invalid task configuration
    #[error("Invalid configuration: {message}")]
    InvalidConfiguration {
        /// Description of the configuration error
        message: String,
    },

    /// Runtime operation failed
    #[error("Runtime operation '{operation}' failed")]
    RuntimeFailure {
        /// Description of what failed
        operation: String,
        /// Underlying error cause
        #[source]
        source: anyhow::Error,
    },

    /// Internal supervisor error (should not occur in normal operation)
    #[error("Internal supervisor error: {message}")]
    InternalError {
        /// Description of the internal error
        message: String,
        /// Optional source error
        #[source]
        source: Option<anyhow::Error>,
    },
}

impl SupervisorError {
    /// Get the error kind for this error
    pub fn kind(&self) -> ErrorKind {
        match self {
            SupervisorError::DependencyValidation { .. } |
            SupervisorError::CircularDependency { .. } |
            SupervisorError::InvalidConfiguration { .. } => ErrorKind::Configuration,
            
            SupervisorError::PrerequisiteFailed { .. } |
            SupervisorError::SetupFailed { .. } |
            SupervisorError::DependencySetupFailed { .. } => ErrorKind::Runtime,
            
            SupervisorError::TaskExecutionFailed { .. } |
            SupervisorError::TaskPanicked { .. } |
            SupervisorError::MaxAttemptsReached { .. } |
            SupervisorError::RestartPrevented { .. } => ErrorKind::Execution,
            
            SupervisorError::RuntimeFailure { .. } |
            SupervisorError::InternalError { .. } => ErrorKind::System,
        }
    }

    /// Check if this is a configuration error
    pub fn is_configuration_error(&self) -> bool {
        matches!(self.kind(), ErrorKind::Configuration)
    }

    /// Check if this is a runtime error
    pub fn is_runtime_error(&self) -> bool {
        matches!(self.kind(), ErrorKind::Runtime)
    }

    /// Check if this is an execution error
    pub fn is_execution_error(&self) -> bool {
        matches!(self.kind(), ErrorKind::Execution)
    }

    /// Check if this is a system error
    pub fn is_system_error(&self) -> bool {
        matches!(self.kind(), ErrorKind::System)
    }

    /// Convert to anyhow::Error for easy propagation
    pub fn into_anyhow(self) -> anyhow::Error {
        self.into()
    }

    /// Wrap this error in an anyhow::Error with additional context
    pub fn context<C>(self, context: C) -> anyhow::Error 
    where
        C: std::fmt::Display + Send + Sync + 'static,
    {
        self.into_anyhow().context(context)
    }

    /// Wrap this error in an anyhow::Error with additional context from a closure
    pub fn with_context<C, F>(self, f: F) -> anyhow::Error
    where
        C: std::fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        self.into_anyhow().context(f())
    }
}

// Conversion from anyhow::Error for convenience
impl From<anyhow::Error> for SupervisorError {
    fn from(error: anyhow::Error) -> Self {
        SupervisorError::RuntimeFailure {
            operation: "unknown".to_string(),
            source: error,
        }
    }
}



// Helper constructors for common error cases
impl SupervisorError {
    /// Create a dependency validation error
    pub fn dependency_validation(
        task_id: impl Into<String>,
        dependency_id: impl Into<String>,
        reason: ValidationError,
    ) -> Self {
        SupervisorError::DependencyValidation {
            task_id: task_id.into(),
            dependency_id: dependency_id.into(),
            reason,
        }
    }

    /// Create a circular dependency error
    pub fn circular_dependency(
        task_id: impl Into<String>,
        dependency_id: impl Into<String>,
    ) -> Self {
        SupervisorError::CircularDependency {
            task_id: task_id.into(),
            dependency_id: dependency_id.into(),
        }
    }

    /// Create a prerequisite failure error
    pub fn prerequisite_failed(
        name: impl Into<String>,
        source: anyhow::Error,
    ) -> Self {
        SupervisorError::PrerequisiteFailed {
            name: name.into(),
            source,
        }
    }

    /// Create a setup failure error
    pub fn setup_failed(
        task_id: impl Into<String>,
        task_name: impl Into<String>,
        source: anyhow::Error,
    ) -> Self {
        SupervisorError::SetupFailed {
            task_id: task_id.into(),
            task_name: task_name.into(),
            source,
        }
    }

    /// Create a dependency setup failure error
    pub fn dependency_setup_failed(
        task_id: impl Into<String>,
        task_name: impl Into<String>,
        dependency_id: impl Into<String>,
        error_message: impl Into<String>,
    ) -> Self {
        SupervisorError::DependencySetupFailed {
            task_id: task_id.into(),
            task_name: task_name.into(),
            dependency_id: dependency_id.into(),
            error_message: error_message.into(),
        }
    }

    /// Create a task execution failure error
    pub fn task_execution_failed(
        task_id: impl Into<String>,
        task_name: impl Into<String>,
        attempt: usize,
        source: anyhow::Error,
    ) -> Self {
        SupervisorError::TaskExecutionFailed {
            task_id: task_id.into(),
            task_name: task_name.into(),
            attempt,
            source,
        }
    }

    /// Create a task panic error
    pub fn task_panicked(
        task_id: impl Into<String>,
        task_name: impl Into<String>,
        attempt: usize,
        panic_message: impl Into<String>,
    ) -> Self {
        SupervisorError::TaskPanicked {
            task_id: task_id.into(),
            task_name: task_name.into(),
            attempt,
            panic_message: panic_message.into(),
        }
    }

    /// Create a max attempts reached error
    pub fn max_attempts_reached(
        task_id: impl Into<String>,
        task_name: impl Into<String>,
        max_attempts: usize,
        actual_attempts: usize,
    ) -> Self {
        SupervisorError::MaxAttemptsReached {
            task_id: task_id.into(),
            task_name: task_name.into(),
            max_attempts,
            actual_attempts,
        }
    }

    /// Create a restart prevented error
    pub fn restart_prevented(
        task_id: impl Into<String>,
        task_name: impl Into<String>,
        attempt: usize,
        reason: impl Into<String>,
    ) -> Self {
        SupervisorError::RestartPrevented {
            task_id: task_id.into(),
            task_name: task_name.into(),
            attempt,
            reason: reason.into(),
        }
    }

    /// Create an invalid configuration error
    pub fn invalid_configuration(message: impl Into<String>) -> Self {
        SupervisorError::InvalidConfiguration {
            message: message.into(),
        }
    }

    /// Create a runtime failure error
    pub fn runtime_failure(
        operation: impl Into<String>,
        source: anyhow::Error,
    ) -> Self {
        SupervisorError::RuntimeFailure {
            operation: operation.into(),
            source,
        }
    }

    /// Create an internal error
    pub fn internal_error(
        message: impl Into<String>,
        source: Option<anyhow::Error>,
    ) -> Self {
        SupervisorError::InternalError {
            message: message.into(),
            source,
        }
    }
}