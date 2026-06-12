use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, Notify};
use tracing::{debug, error, info};

use crate::batch::{BatchConfig, BatchHandler, BatchStatus, MessageBatch, ReceivedBatchMessage};
use crate::error::WorkerResult;
use crate::message::ReceivedMessage;
use crate::middleware::{MessageHandler, Middleware};

/// Internal queued message for batch assembly
struct QueuedMessage {
    received_message: ReceivedMessage<serde_json::Value>,
}

/// Middleware that automatically batches messages before processing.
///
/// This middleware intercepts individual messages and assembles them into batches.
/// When a batch is complete (by size or timeout), it processes all messages together
/// using a BatchHandler, then acknowledges each message individually.
///
/// # Example
/// ```rust,no_run
/// use foxtive_worker::middleware::batch::BatchMiddleware;
/// use foxtive_worker::{BatchConfig, BatchHandler, ReceivedBatchMessage, MessageBatch};
/// use foxtive_worker::error::WorkerResult;
/// use std::sync::Arc;
///
/// // Create a batch handler
/// struct MyBatchHandler;
///
/// #[async_trait::async_trait]
/// impl BatchHandler for MyBatchHandler {
///     async fn process_batch(&self, _batch: MessageBatch<serde_json::Value>) -> WorkerResult<()> {
///         Ok(())
///     }
/// }
///
/// // Configure batching
/// let config = BatchConfig::default()
///     .with_batch_size(10)
///     .with_flush_interval(std::time::Duration::from_secs(5));
///
/// // Create middleware
/// let batch_middleware = BatchMiddleware::new(Arc::new(MyBatchHandler), config);
/// ```
pub struct BatchMiddleware {
    /// The handler that processes completed batches
    handler: Arc<dyn BatchHandler>,

    /// Configuration for batching behavior
    config: BatchConfig,

    /// Queue of messages waiting to be batched
    queue: Arc<Mutex<Vec<QueuedMessage>>>,

    /// Signal when new messages arrive
    notify: Arc<Notify>,

    /// Background task handle
    _task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl BatchMiddleware {
    /// Create a new batch middleware
    pub fn new(handler: Arc<dyn BatchHandler>, config: BatchConfig) -> Self {
        Self {
            handler,
            config,
            queue: Arc::new(Mutex::new(Vec::new())),
            notify: Arc::new(Notify::new()),
            _task_handle: None,
        }
    }

    /// Start the background batch processing task
    pub async fn start(&mut self) -> WorkerResult<()> {
        info!(
            "Starting batch middleware with batch_size={}, flush_interval={:?}",
            self.config.batch_size, self.config.flush_interval
        );

        let queue = self.queue.clone();
        let notify = self.notify.clone();
        let handler = self.handler.clone();
        let config = self.config.clone();

        let task_handle = tokio::spawn(async move {
            Self::processing_loop(queue, notify, handler, config).await;
        });

        self._task_handle = Some(task_handle);

        Ok(())
    }

    /// Add a message to the batch queue
    async fn enqueue_message(
        &self,
        message: ReceivedMessage<serde_json::Value>,
    ) -> WorkerResult<()> {
        let mut queue = self.queue.lock().await;

        let queued_msg = QueuedMessage {
            received_message: message,
        };

        queue.push(queued_msg);

        // Notify the processing loop
        self.notify.notify_one();

        debug!("Message enqueued for batching, queue size: {}", queue.len());

        Ok(())
    }

    /// Main processing loop
    async fn processing_loop(
        queue: Arc<Mutex<Vec<QueuedMessage>>>,
        notify: Arc<Notify>,
        handler: Arc<dyn BatchHandler>,
        config: BatchConfig,
    ) {
        let mut last_flush = Instant::now();

        loop {
            tokio::select! {
                // Wait for new messages
                _ = notify.notified() => {
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

        // Extract messages for the batch
        let batch_messages: Vec<ReceivedBatchMessage<serde_json::Value>> = queue_guard
            .drain(..config.batch_size)
            .enumerate()
            .map(|(idx, qm)| ReceivedBatchMessage {
                message: qm.received_message.message,
                batch_index: idx,
            })
            .collect();

        // Store received messages for acknowledgment after processing
        let received_messages: Vec<ReceivedMessage<serde_json::Value>> = queue_guard
            .iter()
            .take(config.batch_size)
            .map(|qm| qm.received_message.clone())
            .collect();

        drop(queue_guard); // Release lock before processing

        if batch_messages.is_empty() {
            return Ok(());
        }

        let batch_id = format!("batch-{}", uuid::Uuid::new_v4());
        let mut batch = MessageBatch::new(batch_id.clone(), batch_messages);
        batch.metadata.status = BatchStatus::Ready;

        info!(
            "Processing batch {} with {} messages",
            batch_id,
            batch.len()
        );

        // Process the batch
        match handler.process_batch(batch).await {
            Ok(_) => {
                info!("Batch {} processed successfully", batch_id);

                // Acknowledge all messages in the batch
                for received_msg in received_messages {
                    if let Err(e) = received_msg.ack().await {
                        error!("Failed to acknowledge message in batch: {:?}", e);
                    }
                }

                Ok(())
            }
            Err(e) => {
                error!("Batch {} processing failed: {:?}", batch_id, e);

                // Nack all messages in the batch
                for received_msg in received_messages {
                    if let Err(e) = received_msg.nack(true).await {
                        error!("Failed to nack message in batch: {:?}", e);
                    }
                }

                Err(e)
            }
        }
    }

    /// Flush a partial batch (timeout or shutdown)
    async fn flush_partial_batch(
        queue: &Mutex<Vec<QueuedMessage>>,
        handler: &Arc<dyn BatchHandler>,
        _config: &BatchConfig,
        status: BatchStatus,
    ) -> WorkerResult<()> {
        let mut queue_guard = queue.lock().await;

        if queue_guard.is_empty() {
            return Ok(());
        }

        let count = queue_guard.len();
        debug!("Flushing partial batch with {} messages", count);

        // Extract all remaining messages
        let batch_messages: Vec<ReceivedBatchMessage<serde_json::Value>> = queue_guard
            .drain(..)
            .enumerate()
            .map(|(idx, qm)| ReceivedBatchMessage {
                message: qm.received_message.message,
                batch_index: idx,
            })
            .collect();

        let received_messages: Vec<ReceivedMessage<serde_json::Value>> = queue_guard
            .iter()
            .map(|qm| qm.received_message.clone())
            .collect();

        drop(queue_guard);

        if batch_messages.is_empty() {
            return Ok(());
        }

        let batch_id = format!("partial-{}", uuid::Uuid::new_v4());
        let mut batch = MessageBatch::new(batch_id.clone(), batch_messages);
        batch.metadata.status = status.clone();

        info!(
            "Processing partial batch {} with {} messages (status: {:?})",
            batch_id,
            batch.len(),
            status
        );

        // Process the batch
        match handler.process_batch(batch).await {
            Ok(_) => {
                info!("Partial batch {} processed successfully", batch_id);

                for received_msg in received_messages {
                    if let Err(e) = received_msg.ack().await {
                        error!("Failed to acknowledge message in partial batch: {:?}", e);
                    }
                }

                Ok(())
            }
            Err(e) => {
                error!("Partial batch {} processing failed: {:?}", batch_id, e);

                for received_msg in received_messages {
                    if let Err(e) = received_msg.nack(true).await {
                        error!("Failed to nack message in partial batch: {:?}", e);
                    }
                }

                Err(e)
            }
        }
    }
}

#[async_trait::async_trait]
impl Middleware for BatchMiddleware {
    fn name(&self) -> &str {
        "BatchMiddleware"
    }

    async fn handle(
        &self,
        message: ReceivedMessage<serde_json::Value>,
        _next: Box<dyn MessageHandler>,
    ) -> WorkerResult<()> {
        // Enqueue the message for batching instead of processing immediately
        // The 'next' handler is not used because we're intercepting for batching
        self.enqueue_message(message).await
    }
}
