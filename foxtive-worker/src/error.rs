use std::time::Duration;
use thiserror::Error;

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
    WorkerPanic { id: String, panic_info: String },

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
    RetriesExhausted { source: Box<WorkerError> },

    #[error("Message already acknowledged by middleware")]
    AlreadyAcknowledged,

    #[error("Application error: {0}")]
    AppError(#[from] anyhow::Error),
}

/// A type alias for results returned by worker operations.
pub type WorkerResult<T> = Result<T, WorkerError>;

/// Context information for retry decisions.
///
/// This struct provides comprehensive error context to workers when deciding
/// whether a failed message should be retried or sent to the Dead Letter Queue.
/// It preserves references to the original error chain without requiring cloning,
/// ensuring full debugging capability is maintained.
#[derive(Debug, Clone, Copy)]
pub struct RetryInfo<'a> {
    /// The original error that caused the failure.
    /// This preserves the full error chain including backtraces.
    pub error: &'a WorkerError,

    /// Number of retry attempts already made (0-indexed).
    /// 0 means this is the first failure, 1 means first retry failed, etc.
    pub attempt: u32,

    /// Maximum number of retries configured for this message.
    pub max_retries: u32,

    /// Delay before the next retry will be attempted (if requeued).
    pub retry_delay: Option<Duration>,

    /// Whether this message has been marked as a poison pill
    /// (consistently failing across multiple attempts).
    pub is_poison_pill: bool,
}

impl<'a> RetryInfo<'a> {
    /// Create a new RetryInfo with the given error and context.
    pub fn new(error: &'a WorkerError) -> Self {
        Self {
            error,
            attempt: 0,
            max_retries: 0,
            retry_delay: None,
            is_poison_pill: false,
        }
    }

    /// Set the current attempt number.
    pub fn with_attempt(mut self, attempt: u32) -> Self {
        self.attempt = attempt;
        self
    }

    /// Set the maximum retries allowed.
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set the delay before next retry.
    pub fn with_retry_delay(mut self, delay: Duration) -> Self {
        self.retry_delay = Some(delay);
        self
    }

    /// Mark this message as a potential poison pill.
    pub fn with_poison_pill(mut self, is_poison: bool) -> Self {
        self.is_poison_pill = is_poison;
        self
    }

    /// Check if retries are exhausted.
    pub fn retries_exhausted(&self) -> bool {
        self.attempt >= self.max_retries
    }
}

// Note: WorkerError does not implement Clone because it contains non-cloneable types
// (serde_json::Error and anyhow::Error) that preserve important error context.
// Use RetryInfo<'_> to pass error references for inspection without cloning.

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
