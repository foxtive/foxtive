//! Error types for the supervisor system

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SupervisorError {
    #[error("Configuration error for task '{task_id}': {reason}")]
    ConfigurationError { task_id: String, reason: String },

    #[error("Dependency validation failed for task '{task_id}': {dependency_id} - {error:?}")]
    DependencyValidation {
        task_id: String,
        dependency_id: String,
        error: ValidationError,
    },

    #[error("Circular dependency detected between '{task_a}' and '{task_b}'")]
    CircularDependency { task_a: String, task_b: String },

    #[error("Prerequisite '{name}' failed: {error}")]
    PrerequisiteFailed { name: String, error: String },

    #[error("Task '{0}' not found")]
    UnknownTask(String),

    #[error("Internal error: {0}")]
    InternalError(String),
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

    pub fn prerequisite_failed(name: &'static str, error: anyhow::Error) -> Self {
        Self::PrerequisiteFailed {
            name: name.to_string(),
            error: format!("{error:?}"),
        }
    }
}
