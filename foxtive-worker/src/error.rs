use thiserror::Error;
use std::time::Duration;

/// Error types for the foxtive-worker crate.
#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("Message processing failed: {0}")]
    ProcessingFailed(String),

    #[error("Processing error: {0}")]
    ProcessingError(String),

    #[error("Backend error: {0}")]
    BackendError(String),

    #[error("Acknowledgment failed: {0}")]
    AcknowledgmentFailed(String),

    #[error("Worker {id} panicked: {panic_info}")]
    WorkerPanic {
        id: String,
        panic_info: String,
    },

    #[error("Pool exhausted: no available workers")]
    PoolExhausted,

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Shutdown requested")]
    Shutdown,

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Timeout error: {0}")]
    Timeout(String),

    #[error("Channel error: {0}")]
    ChannelError(String),

    #[error("Retryable failure: {source}. Retrying in {delay_ms:?}ms")]
    RetryableFailure {
        source: Box<WorkerError>,
        delay_ms: Duration,
    },

    #[error("Retries exhausted for message: {source}")]
    RetriesExhausted {
        source: Box<WorkerError>,
    },

    #[error("Message already acknowledged by middleware")]
    AlreadyAcknowledged,

    #[error("Application error: {0}")]
    AppError(#[from] anyhow::Error),
}

/// A type alias for results returned by worker operations.
pub type WorkerResult<T> = Result<T, WorkerError>;

// Note: Removed generic From<anyhow::Error> implementation to avoid conflict with AppError variant.
// Use WorkerError::AppError(err) or the ? operator with anyhow::Error directly.
// The AppError variant preserves the full anyhow::Error with context chain and backtrace.

impl From<std::io::Error> for WorkerError {
    fn from(err: std::io::Error) -> Self {
        WorkerError::BackendError(err.to_string())
    }
}

impl From<tokio::task::JoinError> for WorkerError {
    fn from(err: tokio::task::JoinError) -> Self {
        // Preserve panic information if available
        match err.try_into_panic() {
            Ok(reason) => {
                // Task panicked
                if let Some(s) = reason.downcast_ref::<String>() {
                    WorkerError::ProcessingFailed(format!("Task panicked: {}", s))
                } else if let Some(s) = reason.downcast_ref::<&str>() {
                    WorkerError::ProcessingFailed(format!("Task panicked: {}", s))
                } else {
                    WorkerError::ProcessingFailed("Task panicked with unknown reason".to_string())
                }
            }
            Err(cancelled_err) => {
                // Task was cancelled
                WorkerError::ProcessingFailed(format!("Task cancelled: {}", cancelled_err))
            }
        }
    }
}