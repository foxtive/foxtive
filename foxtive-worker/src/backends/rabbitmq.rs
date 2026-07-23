use crate::MessageProperties;
use crate::backends::ReceiveResult;
use crate::backends::contract::MessageBackend;
use crate::error::{WorkerError, WorkerResult};
use crate::message::{AckHandle, Message, MessageMetadata, ReceivedMessage};
use async_trait::async_trait;
use lapin::BasicProperties;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions, BasicQosOptions,
    ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions,
};
use lapin::types::{AMQPValue, FieldTable};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tracing::error;

/// RabbitMQ acknowledgment handle.
///
/// Uses the consume channel for ack/nack operations. Delivery tags are
/// channel-scoped in AMQP, so the ack must go on the same channel that
/// received the delivery.
pub struct RabbitMqAckHandle {
    delivery_tag: u64,
    channel: Arc<Mutex<lapin::Channel>>,
    retry_publisher: Option<Arc<dyn RetryPublisher + Send + Sync>>,
}

impl std::fmt::Debug for RabbitMqAckHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RabbitMqAckHandle")
            .field("delivery_tag", &self.delivery_tag)
            .field(
                "retry_publisher",
                &self.retry_publisher.as_ref().map(|_| "<RetryPublisher>"),
            )
            .finish()
    }
}

#[async_trait]
impl AckHandle for RabbitMqAckHandle {
    async fn ack(&self) -> WorkerResult<()> {
        // Lock the channel to ensure sequential ack operations
        tracing::debug!("Attempting to ack delivery tag {}", self.delivery_tag);
        let channel = self.channel.lock().await;

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
                error!("Failed to ack delivery tag {}: {}", self.delivery_tag, e);
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

        // If requeue is false and we have DLQ configured, message will be discarded
        // The pool layer should have already published to DLQ before calling nack(false)
        let channel = self.channel.lock().await;

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
                error!("Failed to nack delivery tag {}: {}", self.delivery_tag, e);
                WorkerError::BackendError(format!("Failed to nack message: {}", e))
            })?;

        Ok(())
    }

    async fn retry_with_delay(
        &self,
        message: &Message<serde_json::Value>,
        delay_ms: u64,
    ) -> WorkerResult<()> {
        tracing::info!(
            "[RabbitMqAckHandle] retry_with_delay called for message {} with delay {}ms",
            message.id,
            delay_ms
        );

        if let Some(ref publisher) = self.retry_publisher {
            tracing::info!(
                "[RabbitMqAckHandle] Retry publisher available for message {}, attempting delayed retry",
                message.id
            );
            // Use delayed retry via DLX+TTL
            match publisher.publish_retry(message, delay_ms).await {
                Ok(()) => {
                    tracing::info!(
                        "Successfully published message {} to retry queue with {}ms delay",
                        message.id,
                        delay_ms
                    );
                    // Acknowledge the original message since we've republished it
                    self.ack().await
                }
                Err(e) => {
                    error!(
                        "Failed to publish message {} to retry queue: {}. Falling back to immediate nack.",
                        message.id, e
                    );
                    // Fallback to immediate requeue
                    self.nack(true).await
                }
            }
        } else {
            tracing::warn!(
                "[RabbitMqAckHandle] Retry publisher NOT available for message {}. Using immediate nack.",
                message.id
            );
            // Fallback to immediate requeue if retry publisher not available
            self.nack(true).await
        }
    }

    async fn send_to_dlq(
        &self,
        message: &Message<serde_json::Value>,
        error_message: &str,
    ) -> WorkerResult<()> {
        tracing::info!(
            "[RabbitMqAckHandle] send_to_dlq called for message {}",
            message.id
        );

        if let Some(ref backend) = self.retry_publisher {
            if let Some(retry_pub) = backend.as_any().downcast_ref::<RabbitMqRetryPublisher>() {
                match retry_pub.publish_to_dlq(message, error_message).await {
                    Ok(()) => {
                        tracing::info!(
                            "Successfully published message {} to DLQ after retries exhausted",
                            message.id
                        );
                        // Acknowledge the original message since it's now in DLQ
                        self.ack().await
                    }
                    Err(e) => {
                        error!(
                            "Failed to publish message {} to DLQ: {}. Falling back to nack(false).",
                            message.id, e
                        );
                        // Fallback: nack without requeue (message will be discarded)
                        self.nack(false).await
                    }
                }
            } else {
                tracing::warn!(
                    "[RabbitMqAckHandle] retry_publisher is not RabbitMqRetryPublisher, using nack(false)"
                );
                self.nack(false).await
            }
        } else {
            tracing::warn!(
                "[RabbitMqAckHandle] Retry publisher NOT available for message {}. Using nack(false).",
                message.id
            );
            // Fallback: nack without requeue
            self.nack(false).await
        }
    }
}

/// Trait for publishing messages to retry queue
#[async_trait]
pub trait RetryPublisher {
    async fn publish_retry(
        &self,
        message: &Message<serde_json::Value>,
        delay_ms: u64,
    ) -> WorkerResult<()>;

    /// Cast self to Any for downcasting
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Lightweight retry/DLQ publisher for RabbitMQ.
///
/// Used by `RabbitMqAckHandle` to publish messages to the retry queue or DLQ
/// without carrying the full `RabbitMqBackend` state (consumer handle, channels, etc.).
pub struct RabbitMqRetryPublisher {
    pool: deadpool_lapin::Pool,
    config: RabbitMqConsumerConfig,
    retry_queue_name: Option<String>,
    retry_exchange_name: Option<String>,
    dlq_name: Option<String>,
}

impl std::fmt::Debug for RabbitMqRetryPublisher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RabbitMqRetryPublisher")
            .field("queue", &self.config.queue_name)
            .finish()
    }
}

impl RabbitMqRetryPublisher {
    /// Publish a message to the DLQ with failure metadata.
    pub async fn publish_to_dlq(
        &self,
        message: &Message<serde_json::Value>,
        error_message: &str,
    ) -> WorkerResult<()> {
        let dlq_name = self.dlq_name.as_ref().ok_or_else(|| {
            WorkerError::BackendError("DLQ not configured".to_string())
        })?;

        let payload =
            serde_json::to_vec(&message.payload).map_err(WorkerError::SerializationError)?;

        let mut headers = FieldTable::default();
        if let Some(original_rk) = &message.metadata.routing_key {
            headers.insert(
                "x-original-routing-key".into(),
                AMQPValue::LongString(original_rk.clone().into()),
            );
        }
        headers.insert(
            "x-failure-reason".into(),
            AMQPValue::LongString(error_message.into()),
        );
        headers.insert(
            "x-final-attempt".into(),
            AMQPValue::LongInt(message.metadata.attempt as i32),
        );
        use chrono::Utc;
        headers.insert(
            "x-failed-at".into(),
            AMQPValue::LongString(Utc::now().to_rfc3339().into()),
        );

        let properties = BasicProperties::default()
            .with_message_id(message.id.clone().into())
            .with_content_type("application/json".into())
            .with_headers(headers);

        let conn = self.pool.get().await.map_err(|e| {
            WorkerError::BackendError(format!("Failed to get connection for DLQ: {}", e))
        })?;
        let channel = conn.create_channel().await.map_err(|e| {
            WorkerError::BackendError(format!("Failed to create channel for DLQ: {}", e))
        })?;

        channel
            .basic_publish("", dlq_name, BasicPublishOptions::default(), &payload, properties)
            .await
            .map_err(|e| {
                WorkerError::BackendError(format!("Failed to publish to DLQ: {}", e))
            })?;

        tracing::info!(
            "Published message {} to DLQ '{}' after {} failed attempts",
            message.id, dlq_name, message.metadata.attempt
        );
        Ok(())
    }
}

#[async_trait]
impl RetryPublisher for RabbitMqRetryPublisher {
    async fn publish_retry(
        &self,
        message: &Message<serde_json::Value>,
        delay_ms: u64,
    ) -> WorkerResult<()> {
        let retry_queue = self.retry_queue_name.as_ref().ok_or_else(|| {
            WorkerError::BackendError("Retry queue not configured".to_string())
        })?;
        let retry_exchange = self.retry_exchange_name.as_ref().ok_or_else(|| {
            WorkerError::BackendError("Retry exchange not configured".to_string())
        })?.clone();

        let clamped_delay = delay_ms
            .max(self.config.min_retry_delay_ms)
            .min(self.config.max_retry_delay_ms);

        let payload =
            serde_json::to_vec(&message.payload).map_err(WorkerError::SerializationError)?;

        let routing_key = message
            .metadata
            .routing_key
            .as_deref()
            .unwrap_or(&self.config.queue_name);

        let mut headers = FieldTable::default();
        if let Some(original_rk) = &message.metadata.routing_key {
            headers.insert(
                "x-original-routing-key".into(),
                AMQPValue::LongString(original_rk.clone().into()),
            );
        }
        let next_attempt = message.metadata.attempt + 1;
        headers.insert(
            "x-retry-attempt".into(),
            AMQPValue::LongInt(next_attempt as i32),
        );

        let properties = BasicProperties::default()
            .with_message_id(message.id.clone().into())
            .with_content_type("application/json".into())
            .with_expiration(clamped_delay.to_string().into())
            .with_headers(headers);

        let conn = self.pool.get().await.map_err(|e| {
            WorkerError::BackendError(format!("Failed to get connection for retry: {}", e))
        })?;
        let channel = conn.create_channel().await.map_err(|e| {
            WorkerError::BackendError(format!("Failed to create channel for retry: {}", e))
        })?;

        channel
            .basic_publish(
                &retry_exchange,
                routing_key,
                BasicPublishOptions::default(),
                &payload,
                properties,
            )
            .await
            .map_err(|e| {
                WorkerError::BackendError(format!("Failed to publish to retry queue: {}", e))
            })?;

        tracing::info!(
            "Published message {} to retry queue '{}' via exchange '{}' with {}ms delay",
            message.id, retry_queue, retry_exchange, clamped_delay
        );
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
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
    /// Enable delayed retry using Dead Letter Exchange (DLX) with TTL
    /// When enabled, failed messages are published to a retry queue with TTL
    /// instead of being immediately requeued
    pub enable_delayed_retry: bool,
    /// Name of the retry queue (auto-generated if None)
    pub retry_queue_name: Option<String>,
    /// Name of the dead letter exchange for retry queue
    pub retry_exchange_name: Option<String>,
    /// Maximum retry delay in milliseconds (default: 1 hour = 3_600_000ms)
    /// RabbitMQ TTL has no strict maximum, but practical limits apply
    pub max_retry_delay_ms: u64,
    /// Minimum retry delay in milliseconds (default: 1 second = 1_000ms)
    pub min_retry_delay_ms: u64,
    /// Queue declaration options (durable, auto_delete, etc.)
    /// Applied to the main queue, retry queue, and DLQ.
    pub queue_declare_options: QueueDeclareOptions,
    /// Queue declaration arguments (x-queue-type, x-message-ttl, etc.)
    /// Applied to the main queue, retry queue, and DLQ.
    ///
    /// # Example: Quorum queue
    /// ```ignore
    /// use lapin::types::{AMQPValue, FieldTable};
    /// let mut args = FieldTable::default();
    /// args.insert("x-queue-type".into(), AMQPValue::LongString("quorum".into()));
    /// ```
    pub queue_args: FieldTable,
}

impl Default for RabbitMqConsumerConfig {
    fn default() -> Self {
        Self {
            queue_name: "worker_queue".to_string(),
            consumer_tag: "foxtive-worker".to_string(),
            auto_ack: false,
            prefetch_count: 10,
            requeue_on_nack: true,
            enable_delayed_retry: false,
            retry_queue_name: None,
            retry_exchange_name: None,
            max_retry_delay_ms: 3_600_000, // 1 hour
            min_retry_delay_ms: 1_000,     // 1 second
            queue_declare_options: QueueDeclareOptions {
                durable: true,
                ..Default::default()
            },
            queue_args: FieldTable::default(),
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
    message_rx: Arc<Mutex<tokio::sync::mpsc::Receiver<MessageEnvelope>>>,
    pub pool: deadpool_lapin::Pool,
    consume_channel: Arc<Mutex<lapin::Channel>>,
    config: RabbitMqConsumerConfig,
    shutdown_notify: Arc<Notify>,
    _consumer_handle: tokio::task::JoinHandle<()>,
    pub retry_queue_name: Option<String>,
    pub retry_exchange_name: Option<String>,
    pub dlq_name: Option<String>,
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
    pub async fn new(
        amqp_url: impl Into<String>,
        config: RabbitMqConsumerConfig,
    ) -> WorkerResult<Self> {
        // Create connection pool
        let manager =
            deadpool_lapin::Manager::new(amqp_url.into(), lapin::ConnectionProperties::default());

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

        // Declare queue (idempotent) — use user-provided options and args
        consume_channel
            .queue_declare(
                &config.queue_name,
                config.queue_declare_options,
                config.queue_args.clone(),
            )
            .await
            .map_err(|e| WorkerError::BackendError(format!("Failed to declare queue: {}", e)))?;

        // Setup retry infrastructure if delayed retry is enabled
        let (retry_queue_name, retry_exchange_name, dlq_name) = if config.enable_delayed_retry {
            Self::setup_retry_infrastructure(&consume_channel, &config).await?
        } else {
            (None, None, None)
        };

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
                                let mut routing_key = delivery.routing_key.clone();

                                // Track if this is a redelivery and extract attempt count
                                let mut retry_attempt: Option<u32> = None;

                                // Check if we have stored metadata in headers (from retry)
                                if let Some(headers) = delivery.properties.headers() {
                                    // Restore original routing key if present
                                    if let Some(AMQPValue::LongString(original_rk)) = headers.inner().get("x-original-routing-key") {
                                        tracing::debug!(
                                            "Restoring original routing key '{}' from x-original-routing-key header (delivery routing_key was '{}')",
                                            original_rk,
                                            routing_key
                                        );
                                        // Convert LongString to ShortString for routing_key
                                        routing_key = lapin::types::ShortString::from(original_rk.to_string());
                                    }

                                    // Restore retry attempt count if present
                                    if let Some(AMQPValue::LongInt(attempt_val)) = headers.inner().get("x-retry-attempt") {
                                        retry_attempt = Some(*attempt_val as u32);
                                        tracing::debug!(
                                            "Restored retry attempt {} from x-retry-attempt header",
                                            attempt_val
                                        );
                                    }
                                }

                                // Extract message properties from AMQP BasicProperties
                                let mut properties = MessageProperties {
                                    content_type: delivery.properties.content_type()
                                        .as_ref()
                                        .map(|v| v.to_string()),
                                    content_encoding: delivery.properties.content_encoding()
                                        .as_ref()
                                        .map(|v| v.to_string()),
                                    priority: *delivery.properties.priority(),
                                    expiration: delivery.properties.expiration()
                                        .as_ref()
                                        .and_then(|v| v.to_string().parse::<u64>().ok()),
                                    message_type: None, // Not available in lapin (use headers instead)
                                    user_id: delivery.properties.user_id()
                                        .as_ref()
                                        .map(|v| v.to_string()),
                                    app_id: delivery.properties.app_id()
                                        .as_ref()
                                        .map(|v| v.to_string()),
                                    cluster_id: None, // Not available in lapin
                                    reply_to: delivery.properties.reply_to()
                                        .as_ref()
                                        .map(|v| v.to_string()),
                                    headers: None,
                                };

                                // Extract custom headers from FieldTable
                                if let Some(field_table) = delivery.properties.headers() {
                                    let mut headers_map = std::collections::HashMap::new();
                                    // FieldTable has an inner() method that returns the HashMap
                                    for (key, value) in field_table.inner().iter() {
                                        // Convert AMQP values to strings
                                        let value_str = match value {
                                            AMQPValue::ShortString(s) => Some(s.to_string()),
                                            AMQPValue::LongString(s) => Some(s.to_string()),
                                            AMQPValue::LongInt(i) => Some(i.to_string()),
                                            AMQPValue::Timestamp(t) => Some(t.to_string()),
                                            _ => None, // Skip unsupported types
                                        };
                                        if let Some(v) = value_str {
                                            headers_map.insert(key.to_string(), v);
                                        }
                                    }
                                    if !headers_map.is_empty() {
                                        properties.headers = Some(headers_map);
                                    }
                                };

                                // Build metadata with routing key, properties, and restored attempt count
                                tracing::debug!(
                                    "Creating message metadata with routing_key='{}', queue_name='{}'",
                                    routing_key,
                                    queue_name
                                );
                                let mut metadata = MessageMetadata::new(&queue_name)
                                    .with_routing_key(routing_key)
                                    .with_properties(properties);

                                // Restore attempt count from retry headers if present
                                if let Some(attempt) = retry_attempt {
                                    metadata.attempt = attempt;
                                    tracing::info!(
                                        "Restored attempt count {} for redelivered message {}",
                                        attempt,
                                        message_id
                                    );
                                }

                                // Create worker message
                                tracing::info!(
                                    "Created message {} with metadata.routing_key={:?}",
                                    message_id,
                                    metadata.routing_key
                                );
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
            retry_queue_name,
            retry_exchange_name,
            dlq_name,
        })
    }

    /// Create a new backend with default configuration.
    pub async fn with_defaults(amqp_url: &str) -> WorkerResult<Self> {
        Self::new(amqp_url, RabbitMqConsumerConfig::default()).await
    }

    /// Setup retry infrastructure: exchange, retry queue with DLX, and DLQ
    async fn setup_retry_infrastructure(
        channel: &lapin::Channel,
        config: &RabbitMqConsumerConfig,
    ) -> WorkerResult<(Option<String>, Option<String>, Option<String>)> {
        // Generate retry queue and exchange names if not provided
        let retry_queue = config
            .retry_queue_name
            .clone()
            .unwrap_or_else(|| format!("{}_retry", config.queue_name));
        let retry_exchange = config
            .retry_exchange_name
            .clone()
            .unwrap_or_else(|| format!("{}_retry_exchange", config.queue_name));

        tracing::info!(
            "Setting up retry infrastructure: queue={}, exchange={}, dlx={}",
            retry_queue,
            retry_exchange,
            config.queue_name
        );

        // Declare the dead letter exchange as Topic to support wildcard routing
        channel
            .exchange_declare(
                &retry_exchange,
                lapin::ExchangeKind::Topic,
                ExchangeDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|e| {
                WorkerError::BackendError(format!("Failed to declare retry exchange: {}", e))
            })?;

        // Declare the retry queue with DLX pointing back to the main queue
        // When TTL expires, dead-letter to default exchange with queue name as routing key
        let mut args = config.queue_args.clone();
        args.insert(
            "x-dead-letter-exchange".into(),
            AMQPValue::LongString("".into()), // Empty string means default exchange
        );
        // Set routing key to queue name so default exchange routes back to main queue
        args.insert(
            "x-dead-letter-routing-key".into(),
            AMQPValue::LongString(config.queue_name.clone().into()),
        );

        channel
            .queue_declare(
                &retry_queue,
                config.queue_declare_options,
                args,
            )
            .await
            .map_err(|e| {
                WorkerError::BackendError(format!("Failed to declare retry queue: {}", e))
            })?;

        // Bind retry queue to retry exchange
        // Use '#' wildcard to match all routing keys, since messages can have different routing keys
        channel
            .queue_bind(
                &retry_queue,
                &retry_exchange,
                "#", // Match all routing keys
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| WorkerError::BackendError(format!("Failed to bind retry queue: {}", e)))?;

        // Create Dead Letter Queue for exhausted retries — use same options/args as main queue
        let dlq_name = format!("{}-dlq", config.queue_name);
        channel
            .queue_declare(
                &dlq_name,
                config.queue_declare_options,
                config.queue_args.clone(),
            )
            .await
            .map_err(|e| WorkerError::BackendError(format!("Failed to declare DLQ: {}", e)))?;

        tracing::info!(
            "Retry infrastructure setup complete: retry_queue={}, dlq={}",
            retry_queue,
            dlq_name
        );

        Ok((Some(retry_queue), Some(retry_exchange), Some(dlq_name)))
    }

    /// Publish a message to the retry queue with a delay (TTL)
    ///
    /// This method publishes the message to the retry queue with an x-message-ttl header.
    /// When the TTL expires, RabbitMQ will dead-letter the message back to the main queue.
    ///
    /// The delay will be clamped between min_retry_delay_ms and max_retry_delay_ms from config.
    ///
    /// # Arguments
    /// * `message` - The message to retry
    /// * `delay_ms` - Delay in milliseconds before the message should be redelivered
    ///
    /// Will be clamped to [min_retry_delay_ms, max_retry_delay_ms]
    ///
    /// # Returns
    /// Ok if successfully published, Err if publishing failed
    pub async fn publish_to_retry_queue(
        &self,
        message: &Message<serde_json::Value>,
        delay_ms: u64,
    ) -> WorkerResult<()> {
        tracing::info!(
            "[RabbitMqBackend] publish_to_retry_queue called for message {} (requested delay: {}ms)",
            message.id,
            delay_ms
        );

        let retry_queue = self.retry_queue_name.as_ref().ok_or_else(|| {
            error!(
                "[RabbitMqBackend] Retry queue not configured for message {}",
                message.id
            );
            WorkerError::BackendError(
                "Retry queue not configured. Enable delayed retry in config.".to_string(),
            )
        })?;

        let retry_exchange = self
            .retry_exchange_name
            .as_ref()
            .ok_or_else(|| {
                error!(
                    "[RabbitMqBackend] Retry exchange not configured for message {}",
                    message.id
                );
                WorkerError::BackendError(
                    "Retry exchange not configured. Enable delayed retry in config.".to_string(),
                )
            })?
            .clone();

        // Clamp delay between configured min and max
        let clamped_delay = delay_ms
            .max(self.config.min_retry_delay_ms)
            .min(self.config.max_retry_delay_ms);

        if clamped_delay != delay_ms {
            tracing::info!(
                "Clamping retry delay from {}ms to {}ms (range: {}-{}ms)",
                delay_ms,
                clamped_delay,
                self.config.min_retry_delay_ms,
                self.config.max_retry_delay_ms
            );
        }

        // Serialize message payload
        let payload =
            serde_json::to_vec(&message.payload).map_err(WorkerError::SerializationError)?;

        // Preserve the original routing key from message metadata, or fall back to queue name
        tracing::info!(
            "[publish_to_retry_queue] Message {} metadata.routing_key = {:?}, metadata.source = '{}'",
            message.id,
            message.metadata.routing_key,
            message.metadata.source
        );

        let routing_key = message
            .metadata
            .routing_key
            .as_deref()
            .unwrap_or(&self.config.queue_name);

        tracing::debug!(
            "Using routing key '{}' for retry (original: {:?})",
            routing_key,
            message.metadata.routing_key
        );

        // Store original routing key and incremented attempt count in headers so they survive DLX round-trip
        let mut headers = FieldTable::default();
        if let Some(original_rk) = &message.metadata.routing_key {
            headers.insert(
                "x-original-routing-key".into(),
                AMQPValue::LongString(original_rk.clone().into()),
            );
            tracing::debug!(
                "Stored original routing key '{}' in x-original-routing-key header",
                original_rk
            );
        }

        // Increment and store attempt count for retry tracking
        let next_attempt = message.metadata.attempt + 1;
        headers.insert(
            "x-retry-attempt".into(),
            AMQPValue::LongInt(next_attempt as i32),
        );
        tracing::debug!(
            "Stored retry attempt {} in x-retry-attempt header (current: {})",
            next_attempt,
            message.metadata.attempt
        );

        // Set TTL via expiration property with custom headers
        let properties = BasicProperties::default()
            .with_message_id(message.id.clone().into())
            .with_content_type("application/json".into())
            .with_expiration(clamped_delay.to_string().into()) // TTL in milliseconds
            .with_headers(headers);

        // Get a connection from pool for publishing
        let conn = self.pool.get().await.map_err(|e| {
            error!(
                "[RabbitMqBackend] Failed to get connection for retry: {}",
                e
            );
            WorkerError::BackendError(format!("Failed to get connection for retry: {}", e))
        })?;

        let channel = conn.create_channel().await.map_err(|e| {
            error!(
                "[RabbitMqBackend] Failed to create channel for retry: {}",
                e
            );
            WorkerError::BackendError(format!("Failed to create channel for retry: {}", e))
        })?;

        // Publish to retry exchange with the ORIGINAL routing key (preserved from message metadata)
        channel
            .basic_publish(
                &retry_exchange,
                routing_key, // Use original routing key, not queue name
                BasicPublishOptions::default(),
                &payload,
                properties,
            )
            .await
            .map_err(|e| {
                error!("[RabbitMqBackend] Failed to publish to retry queue: {}", e);
                WorkerError::BackendError(format!("Failed to publish to retry queue: {}", e))
            })?;

        tracing::info!(
            "Published message {} to retry queue '{}' via exchange '{}' with {}ms delay",
            message.id,
            retry_queue,
            retry_exchange,
            clamped_delay
        );

        Ok(())
    }

    /// Publish a failed message to the Dead Letter Queue (DLQ) after retries are exhausted
    ///
    /// This method publishes the message to a dedicated DLQ with failure metadata in headers.
    /// The DLQ serves as permanent storage for messages that have failed all retry attempts,
    /// allowing for manual inspection, debugging, or reprocessing.
    ///
    /// # Arguments
    /// * `message` - The message that has exhausted all retries
    /// * `error_message` - Description of why the message failed
    ///
    /// # Returns
    /// Ok if successfully published to DLQ, Err if publishing failed
    pub async fn publish_to_dlq(
        &self,
        message: &Message<serde_json::Value>,
        error_message: &str,
    ) -> WorkerResult<()> {
        let dlq_name = self.dlq_name.as_ref().ok_or_else(|| {
            error!(
                "[RabbitMqBackend] DLQ not configured for message {}",
                message.id
            );
            WorkerError::BackendError(
                "DLQ not configured. Enable delayed retry in config.".to_string(),
            )
        })?;

        // Serialize message payload
        let payload =
            serde_json::to_vec(&message.payload).map_err(WorkerError::SerializationError)?;

        // Create headers with failure metadata
        let mut headers = FieldTable::default();

        // Store original routing key
        if let Some(original_rk) = &message.metadata.routing_key {
            headers.insert(
                "x-original-routing-key".into(),
                AMQPValue::LongString(original_rk.clone().into()),
            );
        }

        // Store failure information
        headers.insert(
            "x-failure-reason".into(),
            AMQPValue::LongString(error_message.into()),
        );

        // Store attempt count
        headers.insert(
            "x-final-attempt".into(),
            AMQPValue::LongInt(message.metadata.attempt as i32),
        );

        // Store timestamp
        use chrono::Utc;
        headers.insert(
            "x-failed-at".into(),
            AMQPValue::LongString(Utc::now().to_rfc3339().into()),
        );

        let properties = BasicProperties::default()
            .with_message_id(message.id.clone().into())
            .with_content_type("application/json".into())
            .with_headers(headers);

        // Get a connection from pool for publishing
        let conn = self.pool.get().await.map_err(|e| {
            error!("[RabbitMqBackend] Failed to get connection for DLQ: {}", e);
            WorkerError::BackendError(format!("Failed to get connection for DLQ: {}", e))
        })?;

        let channel = conn.create_channel().await.map_err(|e| {
            error!("[RabbitMqBackend] Failed to create channel for DLQ: {}", e);
            WorkerError::BackendError(format!("Failed to create channel for DLQ: {}", e))
        })?;

        // Publish to DLQ using default exchange with DLQ name as routing key
        channel
            .basic_publish(
                "",       // Default exchange
                dlq_name, // Routing key = DLQ queue name
                BasicPublishOptions::default(),
                &payload,
                properties,
            )
            .await
            .map_err(|e| {
                error!("[RabbitMqBackend] Failed to publish to DLQ: {}", e);
                WorkerError::BackendError(format!("Failed to publish to DLQ: {}", e))
            })?;

        tracing::info!(
            "Published message {} to DLQ '{}' after {} failed attempts: {}",
            message.id,
            dlq_name,
            message.metadata.attempt,
            error_message
        );

        Ok(())
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
    /// # use foxtive_worker::backends::RabbitMqBackend;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let backend = RabbitMqBackend::with_defaults("amqp://localhost").await?;
    /// // Acknowledge all messages up to tag 1000
    /// backend.batch_ack(1000).await?;
    /// # Ok(())
    /// # }
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
                error!(
                    "Failed to batch ack up to delivery tag {}: {}",
                    delivery_tag, e
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
    /// # use foxtive_worker::backends::RabbitMqBackend;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let backend = RabbitMqBackend::with_defaults("amqp://localhost").await?;
    /// // Increase prefetch for fast workers
    /// backend.adjust_prefetch(50).await?;
    ///
    /// // Decrease prefetch for slow/large messages
    /// backend.adjust_prefetch(5).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn adjust_prefetch(&self, prefetch_count: u16) -> WorkerResult<()> {
        let channel = self.consume_channel.lock().await;

        channel
            .basic_qos(prefetch_count, BasicQosOptions { global: false })
            .await
            .map_err(|e| {
                error!("Failed to adjust prefetch to {}: {}", prefetch_count, e);
                WorkerError::BackendError(format!("Failed to adjust prefetch: {}", e))
            })?;

        tracing::info!("Adjusted prefetch count to {}", prefetch_count);
        Ok(())
    }
}

#[async_trait]
impl RetryPublisher for RabbitMqBackend {
    async fn publish_retry(
        &self,
        message: &Message<serde_json::Value>,
        delay_ms: u64,
    ) -> WorkerResult<()> {
        self.publish_to_retry_queue(message, delay_ms).await
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
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
                // Create ack handle with delivery tag and retry publisher
                let ack_handle = Arc::new(RabbitMqAckHandle {
                    delivery_tag: envelope.delivery_tag,
                    channel: self.consume_channel.clone(),
                    retry_publisher: if self.retry_queue_name.is_some() {
                        Some(Arc::new(RabbitMqRetryPublisher {
                            pool: self.pool.clone(),
                            config: self.config.clone(),
                            retry_queue_name: self.retry_queue_name.clone(),
                            retry_exchange_name: self.retry_exchange_name.clone(),
                            dlq_name: self.dlq_name.clone(),
                        }))
                    } else {
                        None
                    },
                });

                let message = ReceivedMessage::new(envelope.message, ack_handle);
                Ok(ReceiveResult::Message(Box::new(message)))
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
