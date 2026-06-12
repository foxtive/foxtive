use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::error::WorkerResult;
use crate::message::Message;

/// A batch of messages to be processed together.
#[derive(Debug)]
pub struct MessageBatch<T> {
    /// Unique identifier for this batch
    pub id: String,

    /// Messages in this batch
    pub messages: Vec<ReceivedBatchMessage<T>>,

    /// When this batch was created
    pub created_at: std::time::Instant,

    /// Metadata about the batch
    pub metadata: BatchMetadata,
}

impl<T> Clone for MessageBatch<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            messages: self.messages.clone(),
            created_at: self.created_at,
            metadata: self.metadata.clone(),
        }
    }
}

impl<T> MessageBatch<T> {
    /// Create a new message batch
    pub fn new(id: String, messages: Vec<ReceivedBatchMessage<T>>) -> Self {
        let total = messages.len();
        Self {
            id,
            messages,
            created_at: std::time::Instant::now(),
            metadata: BatchMetadata {
                total_messages: total,
                ..Default::default()
            },
        }
    }

    /// Get the number of messages in this batch
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Check if batch is empty
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Get batch age
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }
}

/// Metadata for a message batch
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchMetadata {
    /// Total number of messages in the batch
    pub total_messages: usize,

    /// Source queues represented in this batch
    pub source_queues: Vec<String>,

    /// Batch processing status
    pub status: BatchStatus,

    /// Error information if batch failed
    pub error: Option<String>,
}

/// Status of a batch processing operation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum BatchStatus {
    /// Batch is being assembled
    #[default]
    Assembling,
    /// Batch is ready for processing
    Ready,
    /// Batch is currently being processed
    Processing,
    /// Batch completed successfully
    Completed,
    /// Batch failed
    Failed,
    /// Batch was flushed due to timeout
    TimeoutFlush,
    /// Batch was flushed due to shutdown
    ShutdownFlush,
}

/// A message within a batch context
#[derive(Debug)]
pub struct ReceivedBatchMessage<T> {
    /// The underlying message
    pub message: Message<T>,

    /// Index within the batch
    pub batch_index: usize,
}

impl<T> Clone for ReceivedBatchMessage<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            message: self.message.clone(),
            batch_index: self.batch_index,
        }
    }
}

/// Trait for handlers that process batches of messages
#[async_trait]
pub trait BatchHandler: Send + Sync {
    /// Process a batch of messages
    ///
    /// All messages in the batch should be processed atomically.
    /// If any message fails, the entire batch may need to be retried
    /// or handled according to your error strategy.
    async fn process_batch(&self, batch: MessageBatch<serde_json::Value>) -> WorkerResult<()>;

    /// Optional: Setup before batch handler starts
    async fn setup(&self) -> WorkerResult<()> {
        Ok(())
    }

    /// Optional: Cleanup on shutdown
    async fn teardown(&self) {}

    /// Maximum batch size this handler can process
    fn max_batch_size(&self) -> usize {
        100
    }

    /// Maximum time to wait before flushing an incomplete batch
    fn max_batch_age(&self) -> Duration {
        Duration::from_secs(30)
    }
}

/// Configuration for batch processing
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum number of messages per batch
    pub batch_size: usize,

    /// Maximum time to wait before flushing (even if batch not full)
    pub flush_interval: Duration,

    /// Whether to wait for full batch before processing
    pub wait_for_full_batch: bool,

    /// Timeout for batch processing
    pub processing_timeout: Duration,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            batch_size: 50,
            flush_interval: Duration::from_secs(10),
            wait_for_full_batch: false,
            processing_timeout: Duration::from_secs(60),
        }
    }
}

impl BatchConfig {
    /// Create a new batch config with custom batch size
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Set the flush interval
    pub fn with_flush_interval(mut self, interval: Duration) -> Self {
        self.flush_interval = interval;
        self
    }

    /// Configure whether to wait for full batch
    pub fn wait_for_full_batch(mut self, wait: bool) -> Self {
        self.wait_for_full_batch = wait;
        self
    }

    /// Set processing timeout
    pub fn with_processing_timeout(mut self, timeout: Duration) -> Self {
        self.processing_timeout = timeout;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_batch_creation() {
        let messages = vec![];
        let batch = MessageBatch::<serde_json::Value>::new("batch-1".to_string(), messages);

        assert_eq!(batch.id, "batch-1");
        assert_eq!(batch.len(), 0);
        assert!(batch.is_empty());
    }

    #[test]
    fn test_batch_config_builder() {
        let config = BatchConfig::default()
            .with_batch_size(100)
            .with_flush_interval(Duration::from_secs(5))
            .wait_for_full_batch(true);

        assert_eq!(config.batch_size, 100);
        assert_eq!(config.flush_interval, Duration::from_secs(5));
        assert!(config.wait_for_full_batch);
    }
}
