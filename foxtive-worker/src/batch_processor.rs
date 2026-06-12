use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify};
use tracing::{debug, error, info};

use crate::batch::{BatchConfig, BatchHandler, BatchStatus, MessageBatch, ReceivedBatchMessage};
use crate::error::{WorkerError, WorkerResult};
use crate::message::ReceivedMessage;

/// Internal message wrapper for the batch queue
struct QueuedMessage {
    message: ReceivedMessage<serde_json::Value>,
}

/// Batch processor that assembles messages into batches and processes them
pub struct BatchProcessor {
    /// The handler that processes completed batches
    handler: Arc<dyn BatchHandler>,

    /// Configuration for batching behavior
    config: BatchConfig,

    /// Queue of messages waiting to be batched
    queue: Arc<Mutex<Vec<QueuedMessage>>>,

    /// Signal when new messages arrive
    notify: Arc<Notify>,

    /// Signal for shutdown
    shutdown_notify: Arc<Notify>,

    /// Handle to the background processing task
    _task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl BatchProcessor {
    /// Create a new batch processor
    pub fn new(handler: Arc<dyn BatchHandler>, config: BatchConfig) -> Self {
        Self {
            handler,
            config,
            queue: Arc::new(Mutex::new(Vec::new())),
            notify: Arc::new(Notify::new()),
            shutdown_notify: Arc::new(Notify::new()),
            _task_handle: None,
        }
    }

    /// Start the batch processor background task
    pub async fn start(&mut self) -> WorkerResult<()> {
        info!(
            "Starting batch processor with batch_size={}, flush_interval={:?}",
            self.config.batch_size, self.config.flush_interval
        );

        let queue = self.queue.clone();
        let notify = self.notify.clone();
        let shutdown_notify = self.shutdown_notify.clone();
        let handler = self.handler.clone();
        let config = self.config.clone();

        let task_handle = tokio::spawn(async move {
            Self::processing_loop(queue.clone(), notify, shutdown_notify, handler, config).await;
        });

        self._task_handle = Some(task_handle);

        Ok(())
    }

    /// Add a message to the batch queue
    pub async fn enqueue(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        let mut queue = self.queue.lock().await;

        let queued_msg = QueuedMessage { message };

        queue.push(queued_msg);

        // Notify the processing loop that new messages are available
        self.notify.notify_one();

        debug!("Message enqueued, queue size: {}", queue.len());

        Ok(())
    }

    /// Shutdown the batch processor gracefully
    pub async fn shutdown(&self) -> WorkerResult<()> {
        info!("Shutting down batch processor...");

        // Signal shutdown
        self.shutdown_notify.notify_one();

        // Process any remaining messages in the queue
        self.flush_remaining().await?;

        Ok(())
    }

    /// Flush any remaining messages in the queue
    async fn flush_remaining(&self) -> WorkerResult<()> {
        let mut queue = self.queue.lock().await;

        if !queue.is_empty() {
            let count = queue.len();
            info!("Flushing {} remaining messages before shutdown", count);

            // Create a batch from remaining messages
            let batch_messages: Vec<ReceivedBatchMessage<serde_json::Value>> = queue
                .drain(..)
                .enumerate()
                .map(|(idx, qm)| ReceivedBatchMessage {
                    message: qm.message.message,
                    batch_index: idx,
                })
                .collect();

            drop(queue); // Release lock before processing

            if !batch_messages.is_empty() {
                let batch_id = format!("flush-{}", uuid::Uuid::new_v4());
                let batch = MessageBatch::new(batch_id, batch_messages);

                match self.process_batch_with_retry(&batch).await {
                    Ok(_) => {
                        info!("Successfully flushed {} messages", count);
                    }
                    Err(e) => {
                        error!("Failed to flush remaining messages: {:?}", e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Main processing loop
    async fn processing_loop(
        queue: Arc<Mutex<Vec<QueuedMessage>>>,
        notify: Arc<Notify>,
        shutdown_notify: Arc<Notify>,
        handler: Arc<dyn BatchHandler>,
        config: BatchConfig,
    ) {
        let mut last_flush = Instant::now();

        loop {
            tokio::select! {
                // Wait for new messages or shutdown signal
                _ = notify.notified() => {
                    // Check if we have enough messages to form a batch
                    let queue_len = queue.lock().await.len();

                    if queue_len >= config.batch_size {
                        // Process a full batch
                        if let Err(e) = Self::process_full_batch(&queue, &handler, &config).await {
                            error!("Failed to process batch: {:?}", e);
                        }
                        last_flush = Instant::now();
                    }
                }

                // Periodic flush based on timeout
                _ = tokio::time::sleep(config.flush_interval) => {
                    if !config.wait_for_full_batch {
                        let elapsed = last_flush.elapsed();

                        if elapsed >= config.flush_interval {
                            debug!("Flush interval reached, checking for partial batch");

                            if let Err(e) = Self::flush_partial_batch(&queue, &handler, &config, BatchStatus::TimeoutFlush).await {
                                error!("Failed to flush partial batch: {:?}", e);
                            }
                            last_flush = Instant::now();
                        }
                    }
                }

                // Shutdown signal
                _ = shutdown_notify.notified() => {
                    info!("Batch processor received shutdown signal");
                    break;
                }
            }
        }
    }

    /// Process a full batch when queue reaches batch_size
    async fn process_full_batch(
        queue: &Mutex<Vec<QueuedMessage>>,
        handler: &Arc<dyn BatchHandler>,
        config: &BatchConfig,
    ) -> WorkerResult<()> {
        let mut queue_guard = queue.lock().await;

        if queue_guard.len() < config.batch_size {
            return Ok(());
        }

        // Extract batch_size messages
        let batch_data: Vec<QueuedMessage> = queue_guard.drain(..config.batch_size).collect();
        drop(queue_guard); // Release lock

        // Convert to batch messages
        let batch_messages: Vec<ReceivedBatchMessage<serde_json::Value>> = batch_data
            .into_iter()
            .enumerate()
            .map(|(idx, qm)| ReceivedBatchMessage {
                message: qm.message.message,
                batch_index: idx,
            })
            .collect();

        let batch_id = format!("batch-{}", uuid::Uuid::new_v4());
        let batch = MessageBatch::new(batch_id, batch_messages);

        info!(
            "Processing full batch {} with {} messages",
            batch.id,
            batch.len()
        );

        Self::process_batch_with_timeout(&batch, handler, config.processing_timeout).await
    }

    /// Flush a partial batch (timeout or shutdown)
    async fn flush_partial_batch(
        queue: &Arc<Mutex<Vec<QueuedMessage>>>,
        handler: &Arc<dyn BatchHandler>,
        config: &BatchConfig,
        status: BatchStatus,
    ) -> WorkerResult<()> {
        let mut queue_guard = queue.lock().await;

        if queue_guard.is_empty() {
            return Ok(());
        }

        // Extract all messages
        let batch_data: Vec<QueuedMessage> = queue_guard.drain(..).collect();
        drop(queue_guard);

        let batch_messages: Vec<ReceivedBatchMessage<serde_json::Value>> = batch_data
            .into_iter()
            .enumerate()
            .map(|(idx, qm)| ReceivedBatchMessage {
                message: qm.message.message,
                batch_index: idx,
            })
            .collect();

        let batch_id = format!("partial-{}", uuid::Uuid::new_v4());
        let mut batch = MessageBatch::new(batch_id, batch_messages);
        batch.metadata.status = status.clone();

        info!(
            "Flushing partial batch {} with {} messages (reason: {:?})",
            batch.id,
            batch.len(),
            status
        );

        Self::process_batch_with_timeout(&batch, handler, config.processing_timeout).await
    }

    /// Process a batch with timeout protection
    async fn process_batch_with_timeout(
        batch: &MessageBatch<serde_json::Value>,
        handler: &Arc<dyn BatchHandler>,
        timeout: Duration,
    ) -> WorkerResult<()> {
        match tokio::time::timeout(timeout, handler.process_batch(batch.clone())).await {
            Ok(result) => result,
            Err(_) => Err(WorkerError::ProcessingFailed(format!(
                "Batch {} processing timed out after {:?}",
                batch.id, timeout
            ))),
        }
    }

    /// Process a batch with retry logic
    async fn process_batch_with_retry(
        &self,
        batch: &MessageBatch<serde_json::Value>,
    ) -> WorkerResult<()> {
        // For now, just process once. Could add retry logic here later.
        Self::process_batch_with_timeout(batch, &self.handler, self.config.processing_timeout).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{AckHandle, Message, MessageMetadata};
    use async_trait::async_trait;

    #[derive(Debug)]
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

    struct TestBatchHandler;

    #[async_trait]
    impl BatchHandler for TestBatchHandler {
        async fn process_batch(&self, batch: MessageBatch<serde_json::Value>) -> WorkerResult<()> {
            println!("Processed batch {} with {} messages", batch.id, batch.len());
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_batch_processor_creation() {
        let handler = Arc::new(TestBatchHandler);
        let config = BatchConfig::default();
        let processor = BatchProcessor::new(handler, config);

        assert_eq!(processor.config.batch_size, 50);
    }

    #[tokio::test]
    async fn test_enqueue_message() {
        let handler = Arc::new(TestBatchHandler);
        let config = BatchConfig::default();
        let processor = BatchProcessor::new(handler, config);

        let message = ReceivedMessage::new(
            Message {
                id: "test-1".to_string(),
                payload: serde_json::json!({"test": "data"}),
                metadata: MessageMetadata::new("test-queue"),
            },
            Arc::new(MockAckHandle),
        );

        processor.enqueue(message).await.unwrap();

        let queue_len = processor.queue.lock().await.len();
        assert_eq!(queue_len, 1);
    }
}
