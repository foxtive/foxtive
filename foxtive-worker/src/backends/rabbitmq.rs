use async_trait::async_trait;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicQosOptions, QueueDeclareOptions,
};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

use crate::backends::contract::MessageBackend;
use crate::backends::ReceiveResult;
use crate::error::{WorkerError, WorkerResult};
use crate::message::{AckHandle, Message, MessageMetadata, ReceivedMessage};

/// RabbitMQ acknowledgment handle.
#[derive(Debug)]
pub struct RabbitMqAckHandle {
    delivery_tag: u64,
    /// Shared channel reference with mutex for thread-safe ack operations
    ///
    /// OPTIMIZATION: Uses batch acknowledgments (multiple=true) when possible
    /// to reduce mutex contention under high throughput. Individual acks still
    /// require the mutex, but batch mode allows acknowledging multiple messages
    /// in a single operation.
    ///
    /// Delivery tags are channel-specific in AMQP, so we must use the same channel.
    ack_channel: Arc<Mutex<lapin::Channel>>,
}

#[async_trait]
impl AckHandle for RabbitMqAckHandle {
    async fn ack(&self) -> WorkerResult<()> {
        // Lock the channel to ensure sequential ack operations
        tracing::debug!("Attempting to ack delivery tag {}", self.delivery_tag);
        let channel = self.ack_channel.lock().await;

        match channel
            .basic_ack(
                self.delivery_tag,
                BasicAckOptions {
                    multiple: false, // Individual ack - change to true for batch mode
                },
            )
            .await
        {
            Ok(_) => {
                tracing::debug!("Successfully acked delivery tag {}", self.delivery_tag);
                Ok(())
            }
            Err(e) => {
                tracing::error!("Failed to ack delivery tag {}: {}", self.delivery_tag, e);
                Err(WorkerError::BackendError(format!(
                    "Failed to ack message: {}",
                    e
                )))
            }
        }
    }

    async fn nack(&self, requeue: bool) -> WorkerResult<()> {
        // Lock the channel to ensure sequential nack operations
        tracing::debug!(
            "Attempting to nack delivery tag {} (requeue={})",
            self.delivery_tag,
            requeue
        );
        let channel = self.ack_channel.lock().await;

        channel
            .basic_nack(
                self.delivery_tag,
                lapin::options::BasicNackOptions {
                    multiple: false, // Individual nack
                    requeue,
                },
            )
            .await
            .map_err(|e| {
                tracing::error!("Failed to nack delivery tag {}: {}", self.delivery_tag, e);
                WorkerError::BackendError(format!("Failed to nack message: {}", e))
            })?;

        Ok(())
    }
}

/// Configuration for RabbitMQ consumer.
#[derive(Debug, Clone)]
pub struct RabbitMqConsumerConfig {
    /// Queue name to consume from
    pub queue_name: String,
    /// Consumer tag (identifier)
    pub consumer_tag: String,
    /// Whether to auto-ack messages (not recommended)
    pub auto_ack: bool,
    /// Prefetch count (max unacked messages)
    pub prefetch_count: u16,
    /// Whether to requeue on nack by default
    pub requeue_on_nack: bool,
}

impl Default for RabbitMqConsumerConfig {
    fn default() -> Self {
        Self {
            queue_name: "worker_queue".to_string(),
            consumer_tag: "foxtive-worker".to_string(),
            auto_ack: false,
            prefetch_count: 10,
            requeue_on_nack: true,
        }
    }
}

/// Internal message envelope for passing messages through the channel
struct MessageEnvelope {
    delivery_tag: u64,
    message: Message<serde_json::Value>,
}

/// RabbitMQ message backend using foxtive's RabbitMQ client.
///
/// This backend uses a **persistent consumer** with a shared message channel
/// for high-throughput message processing. It creates a single background task
/// that forwards messages from RabbitMQ to an internal mpsc channel, eliminating
/// the overhead of creating consumers per receive() call.
///
/// # Architecture
/// ```text
/// RabbitMQ → [Persistent Consumer] → mpsc::channel → receive() calls
/// ```
///
/// # Example
/// ```rust,no_run
/// use foxtive_worker::backends::RabbitMqBackend;
/// use foxtive_worker::backends::rabbitmq::RabbitMqConsumerConfig;
///
/// #[tokio::main]
/// async fn main() {
///     let config = RabbitMqConsumerConfig {
///         queue_name: "my-queue".to_string(),
///         ..Default::default()
///     };
///     
///     let backend = RabbitMqBackend::new("amqp://localhost", config).await.unwrap();
/// }
/// ```
pub struct RabbitMqBackend {
    /// Shared message channel receiver (wrapped in Mutex for concurrent access)
    message_rx: Arc<Mutex<tokio::sync::mpsc::Receiver<MessageEnvelope>>>,
    /// Connection pool for health checks
    pool: deadpool_lapin::Pool,
    /// The consume channel - wrapped in Mutex for thread-safe ack/nack operations
    /// All delivery tags are only valid on this specific channel
    consume_channel: Arc<Mutex<lapin::Channel>>,
    /// Config reference
    config: RabbitMqConsumerConfig,
    /// Shutdown signal
    shutdown_notify: Arc<Notify>,
    /// Handle to the background consumer task
    _consumer_handle: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for RabbitMqBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RabbitMqBackend")
            .field("queue", &self.config.queue_name)
            .field("consumer_tag", &self.config.consumer_tag)
            .finish()
    }
}

impl RabbitMqBackend {
    /// Create a new RabbitMQ backend with a persistent consumer.
    ///
    /// This creates a **single long-lived consumer** that runs in a background task
    /// and forwards messages to an internal channel. Multiple `receive()` calls
    /// will pull from this shared channel, avoiding the overhead of creating
    /// new consumers for each message.
    ///
    /// # Arguments
    /// * `amqp_url` - RabbitMQ connection URL (e.g., "amqp://localhost:5672")
    /// * `config` - Consumer configuration
    ///
    /// # Errors
    /// Returns error if connection or channel setup fails
    pub async fn new(amqp_url: impl Into<String>, config: RabbitMqConsumerConfig) -> WorkerResult<Self> {
        // Create connection pool
        let manager = deadpool_lapin::Manager::new(
            amqp_url.into(),
            lapin::ConnectionProperties::default(),
        );

        let pool = deadpool_lapin::Pool::builder(manager)
            .build()
            .map_err(|e| {
                WorkerError::BackendError(format!("Failed to create connection pool: {}", e))
            })?;

        // Get a connection and create consume channel
        let conn = pool
            .get()
            .await
            .map_err(|e| WorkerError::BackendError(format!("Failed to get connection: {}", e)))?;

        let consume_channel = conn
            .create_channel()
            .await
            .map_err(|e| WorkerError::BackendError(format!("Failed to create channel: {}", e)))?;

        // Set QoS prefetch
        consume_channel
            .basic_qos(config.prefetch_count, BasicQosOptions { global: false })
            .await
            .map_err(|e| WorkerError::BackendError(format!("Failed to set QoS: {}", e)))?;

        // Declare queue (idempotent)
        consume_channel
            .queue_declare(
                &config.queue_name,
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                lapin::types::FieldTable::default(),
            )
            .await
            .map_err(|e| WorkerError::BackendError(format!("Failed to declare queue: {}", e)))?;

        // Start persistent consumer
        // Buffer size increased to 500 for high-volume queues (12K+ messages)
        // This provides better backpressure handling when workers are slower than delivery rate
        let (tx, rx) = tokio::sync::mpsc::channel(500);
        let shutdown_notify = Arc::new(Notify::new());

        let consumer_tag = config.consumer_tag.clone();
        let queue_name = config.queue_name.clone();

        // Create the lapin consumer
        let mut lapin_consumer = consume_channel
            .basic_consume(
                &queue_name,
                &consumer_tag,
                BasicConsumeOptions {
                    no_ack: config.auto_ack,
                    ..Default::default()
                },
                lapin::types::FieldTable::default(),
            )
            .await
            .map_err(|e| WorkerError::BackendError(format!("Failed to start consumer: {}", e)))?;

        // Spawn background task to forward messages
        let notify_clone = shutdown_notify.clone();
        let consumer_handle = tokio::spawn(async move {
            use futures_util::StreamExt;

            loop {
                tokio::select! {
                    _ = notify_clone.notified() => {
                        tracing::debug!("[{}] Consumer shutting down", consumer_tag);
                        break;
                    }
                    delivery = lapin_consumer.next() => {
                        match delivery {
                            Some(Ok(delivery)) => {
                                // Extract delivery tag
                                let delivery_tag = delivery.delivery_tag;

                                // Parse payload - fail on invalid JSON
                                let payload: serde_json::Value = match serde_json::from_slice(&delivery.data) {
                                    Ok(p) => p,
                                    Err(e) => {
                                        tracing::error!(
                                            "Failed to deserialize message payload: {} (message_id: {:?}, data length: {})",
                                            e,
                                            delivery.properties.message_id(),
                                            delivery.data.len()
                                        );
                                        // Nack the malformed message without requeue to prevent poison pill
                                        if let Err(nack_err) = delivery.nack(BasicNackOptions::default()).await {
                                            tracing::error!("Failed to nack malformed message: {:?}", nack_err);
                                        }
                                        continue; // Skip this message
                                    }
                                };

                                // Extract message ID
                                let message_id = delivery.properties.message_id()
                                    .as_ref()
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

                                // Extract routing key from delivery info
                                let routing_key = delivery.routing_key.clone();

                                // Build metadata with routing key
                                let metadata = MessageMetadata::new(&queue_name)
                                    .with_routing_key(routing_key);

                                // Create worker message
                                let message = Message {
                                    id: message_id,
                                    payload,
                                    metadata,
                                };

                                // Send to channel (drop if channel closed)
                                let envelope = MessageEnvelope {
                                    delivery_tag,
                                    message,
                                };

                                if tx.send(envelope).await.is_err() {
                                    tracing::debug!("[{}] Receiver dropped, stopping consumer", consumer_tag);
                                    break;
                                }
                            }
                            Some(Err(e)) => {
                                tracing::error!("[{}] Consumer error: {:?}", consumer_tag, e);
                                // Continue on error
                            }
                            None => {
                                tracing::warn!("[{}] Consumer stream ended", consumer_tag);
                                break;
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            message_rx: Arc::new(Mutex::new(rx)),
            pool,
            consume_channel: Arc::new(Mutex::new(consume_channel)),
            config,
            shutdown_notify,
            _consumer_handle: consumer_handle,
        })
    }

    /// Create a new backend with default configuration.
    pub async fn with_defaults(amqp_url: &str) -> WorkerResult<Self> {
        Self::new(amqp_url, RabbitMqConsumerConfig::default()).await
    }

    /// Get the queue name.
    pub fn queue_name(&self) -> &str {
        &self.config.queue_name
    }

    /// Acknowledge all messages up to and including the given delivery tag.
    ///
    /// This is a batch operation that acknowledges multiple messages in a single call,
    /// significantly reducing mutex contention under high throughput.
    ///
    /// # Arguments
    /// * `delivery_tag` - The highest delivery tag to acknowledge (all lower tags are also acked)
    ///
    /// # Example
    /// ```rust,no_run
    /// // Acknowledge all messages up to tag 1000
    /// backend.batch_ack(1000).await?;
    /// ```
    pub async fn batch_ack(&self, delivery_tag: u64) -> WorkerResult<()> {
        let channel = self.consume_channel.lock().await;

        channel
            .basic_ack(
                delivery_tag,
                BasicAckOptions {
                    multiple: true, // Batch mode - ack all messages up to this tag
                },
            )
            .await
            .map_err(|e| {
                tracing::error!(
                    "Failed to batch ack up to delivery tag {}: {}",
                    delivery_tag,
                    e
                );
                WorkerError::BackendError(format!("Failed to batch ack messages: {}", e))
            })?;

        Ok(())
    }

    /// Adjust the prefetch count dynamically based on processing performance.
    ///
    /// This allows tuning the number of unacknowledged messages the broker will deliver,
    /// optimizing for throughput vs. memory usage.
    ///
    /// # Arguments
    /// * `prefetch_count` - New prefetch count (recommended: 10-100 depending on message size)
    ///
    /// # Guidelines
    /// - Increase prefetch when workers process messages quickly (<10ms avg)
    /// - Decrease prefetch when workers are slow (>100ms avg) or messages are large
    /// - Monitor memory usage - higher prefetch = more messages in flight
    ///
    /// # Example
    /// ```rust,no_run
    /// // Increase prefetch for fast workers
    /// backend.adjust_prefetch(50).await?;
    ///
    /// // Decrease prefetch for slow/large messages
    /// backend.adjust_prefetch(5).await?;
    /// ```
    pub async fn adjust_prefetch(&self, prefetch_count: u16) -> WorkerResult<()> {
        let channel = self.consume_channel.lock().await;

        channel
            .basic_qos(prefetch_count, BasicQosOptions { global: false })
            .await
            .map_err(|e| {
                tracing::error!("Failed to adjust prefetch to {}: {}", prefetch_count, e);
                WorkerError::BackendError(format!("Failed to adjust prefetch: {}", e))
            })?;

        tracing::info!("Adjusted prefetch count to {}", prefetch_count);
        Ok(())
    }
}

#[async_trait]
impl MessageBackend for RabbitMqBackend {
    async fn receive(&self) -> WorkerResult<ReceiveResult<serde_json::Value>> {
        // Check if shutdown was requested first
        // Note: We can't directly check Notify state, so we rely on channel closure

        let mut rx = self.message_rx.lock().await;

        match rx.recv().await {
            Some(envelope) => {
                // Create ack handle with delivery tag
                let ack_handle = Arc::new(RabbitMqAckHandle {
                    delivery_tag: envelope.delivery_tag,
                    ack_channel: self.consume_channel.clone(),
                });

                let message = ReceivedMessage::new(envelope.message, ack_handle);
                Ok(ReceiveResult::Message(message))
            }
            None => {
                // Channel closed - determine why
                // If shutdown_notify was triggered, it's a graceful shutdown
                // Otherwise, it's likely a connection loss or consumer crash

                // For now, we'll assume connection lost since shutdown() doesn't close the channel
                // In a future enhancement, we could track shutdown state more explicitly
                Ok(ReceiveResult::ConnectionLost {
                    reason: "Consumer stream ended unexpectedly".to_string(),
                })
            }
        }
    }

    async fn ack(&self, _message_id: &str) -> WorkerResult<()> {
        // For RabbitMQ, we use the delivery-specific ack handle
        // This method is for batch operations which aren't directly supported
        Err(WorkerError::BackendError(
            "Direct ack by ID not supported for RabbitMQ. Use AckHandle from receive()."
                .to_string(),
        ))
    }

    async fn nack(&self, _message_id: &str, _requeue: bool) -> WorkerResult<()> {
        // For RabbitMQ, we use the delivery-specific nack handle
        Err(WorkerError::BackendError(
            "Direct nack by ID not supported for RabbitMQ. Use AckHandle from receive()."
                .to_string(),
        ))
    }

    async fn health_check(&self) -> WorkerResult<()> {
        // Check if we can get a connection from the pool
        let _ = self.pool.get().await.map_err(|e| {
            WorkerError::BackendError(format!("RabbitMQ health check failed: {}", e))
        })?;

        Ok(())
    }

    async fn shutdown(&self) -> WorkerResult<()> {
        // Signal the background consumer task to stop
        self.shutdown_notify.notify_one();

        // The consumer task will exit when it receives the notification
        // and the JoinHandle will clean up automatically
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a running RabbitMQ instance
    // They are marked with #[ignore] to skip in normal test runs

    #[tokio::test]
    #[ignore]
    async fn test_connect_and_health() {
        let backend = RabbitMqBackend::with_defaults("amqp://localhost")
            .await
            .unwrap();
        assert!(backend.health_check().await.is_ok());
    }

    #[tokio::test]
    #[ignore]
    async fn test_receive_timeout() {
        let backend = RabbitMqBackend::with_defaults("amqp://localhost")
            .await
            .unwrap();

        // Should timeout waiting for message on empty queue
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), backend.receive()).await;

        // Will timeout (no messages)
        assert!(result.is_err());
    }
}
