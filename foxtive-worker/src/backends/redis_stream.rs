use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::backends::ReceiveResult;
use crate::backends::contract::MessageBackend;
use crate::error::{WorkerError, WorkerResult};
use crate::message::{AckHandle, Message, MessageMetadata, ReceivedMessage};

/// Redis Streams acknowledgment handle.
#[derive(Debug)]
pub struct RedisStreamAckHandle {
    stream_name: String,
    group_name: String,
    message_id: String,
    redis: deadpool_redis::Pool,
    // Add a reference to the backend to call its DLQ method
    backend: Arc<RedisStreamBackend>,
}

type StreamMessages = Vec<Vec<(String, Vec<(String, String)>)>>;

#[async_trait]
impl AckHandle for RedisStreamAckHandle {
    async fn ack(&self) -> WorkerResult<()> {
        let mut conn = self.redis.get().await.map_err(|e| {
            WorkerError::BackendError(format!("Failed to get Redis connection: {}", e))
        })?;

        // Acknowledge the message in the consumer group
        redis::cmd("XACK")
            .arg(&self.stream_name)
            .arg(&self.group_name)
            .arg(&self.message_id)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| WorkerError::BackendError(format!("Failed to ack message: {}", e)))?;

        Ok(())
    }

    async fn nack(&self, requeue: bool) -> WorkerResult<()> {
        if !requeue {
            // If not requeue, move to DLQ and then ACK from main stream
            match self.backend.get_message_payload(&self.message_id).await {
                Ok(Some(payload)) => {
                    if let Err(dlq_err) = self.backend.add_to_dlq(payload).await {
                        tracing::error!(
                            "Failed to add message {} to DLQ: {} - will ACK without DLQ",
                            self.message_id,
                            dlq_err
                        );
                    } else {
                        tracing::info!("Message {} moved to DLQ successfully", self.message_id);
                    }
                }
                Ok(None) => {
                    tracing::warn!(
                        "Message {} not found for DLQ - may have been deleted",
                        self.message_id
                    );
                }
                Err(e) => {
                    // Message payload retrieval failed (e.g., deserialization error)
                    // Still ACK to prevent poison pill, but log the error
                    tracing::error!(
                        "Failed to retrieve message {} for DLQ: {} - will ACK without DLQ",
                        self.message_id,
                        e
                    );
                }
            }

            // Always ACK from main stream to prevent infinite retry loop
            if let Err(ack_err) = self.ack().await {
                tracing::error!(
                    "Failed to ACK message {} after DLQ attempt: {}",
                    self.message_id,
                    ack_err
                );
                return Err(ack_err);
            }
        } else {
            // If requeue, message remains in PEL
            tracing::warn!(
                "nack called on Redis Stream message {} with requeue=true - message will remain pending",
                self.message_id
            );
        }
        Ok(())
    }
}

/// Configuration for Redis Streams consumer.
#[derive(Debug, Clone)]
pub struct RedisStreamConsumerConfig {
    /// Stream name to consume from
    pub stream_name: String,
    /// Consumer group name
    pub group_name: String,
    /// Consumer name (unique within group)
    pub consumer_name: String,
    /// Block timeout in milliseconds (0 = no block)
    pub block_ms: u64,
    /// Maximum messages to read per call
    pub count: usize,
    /// Whether to acknowledge automatically
    pub auto_ack: bool,
    /// Optional DLQ stream name
    pub dlq_stream_name: Option<String>,
}

impl Default for RedisStreamConsumerConfig {
    fn default() -> Self {
        Self {
            stream_name: "worker_stream".to_string(),
            group_name: "worker_group".to_string(),
            consumer_name: "consumer-1".to_string(),
            block_ms: 1000,
            count: 1,
            auto_ack: false,
            dlq_stream_name: None,
        }
    }
}

/// Redis Streams message backend.
///
/// This backend consumes messages from a Redis Stream using consumer groups
/// for reliable message processing with acknowledgment.
///
/// # Example
/// ```rust,no_run
/// use foxtive_worker::backends::RedisStreamBackend;
/// use foxtive_worker::backends::redis_stream::RedisStreamConsumerConfig;
///
/// #[tokio::main]
/// async fn main() {
///     let config = RedisStreamConsumerConfig {
///         stream_name: "my-stream".to_string(),
///         ..Default::default()
///     };
///     
///     let backend = RedisStreamBackend::new("redis://localhost", config).await.unwrap();
/// }
/// ```
pub struct RedisStreamBackend {
    redis: deadpool_redis::Pool,
    config: RedisStreamConsumerConfig,
    shutdown: Arc<Mutex<bool>>,
}

impl std::fmt::Debug for RedisStreamBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisStreamBackend")
            .field("stream", &self.config.stream_name)
            .field("group", &self.config.group_name)
            .field("consumer", &self.config.consumer_name)
            .finish()
    }
}

impl RedisStreamBackend {
    /// Create a new Redis Streams backend.
    ///
    /// # Arguments
    /// * `redis_url` - Redis connection URL (e.g., "redis://localhost:6379")
    /// * `config` - Consumer configuration
    ///
    /// # Errors
    /// Returns error if connection or stream setup fails
    pub async fn new(redis_url: &str, config: RedisStreamConsumerConfig) -> WorkerResult<Self> {
        // Create Redis pool
        let redis_config = deadpool_redis::Config::from_url(redis_url);
        let redis = redis_config
            .create_pool(Some(deadpool_redis::Runtime::Tokio1))
            .map_err(|e| {
                WorkerError::BackendError(format!("Failed to create Redis pool: {}", e))
            })?;

        // Test connection
        let mut conn = redis
            .get()
            .await
            .map_err(|e| WorkerError::BackendError(format!("Failed to connect to Redis: {}", e)))?;

        redis::cmd("PING")
            .query_async::<String>(&mut conn)
            .await
            .map_err(|e| WorkerError::BackendError(format!("Redis PING failed: {}", e)))?;

        // Create consumer group (idempotent) for main stream
        redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(&config.stream_name)
            .arg(&config.group_name)
            .arg("$") // Start from end
            .arg("MKSTREAM") // Create stream if not exists
            .query_async::<()>(&mut conn)
            .await
            .ok(); // Ignore error if group already exists

        // Create consumer group for DLQ stream if configured
        if let Some(dlq_stream) = &config.dlq_stream_name {
            redis::cmd("XGROUP")
                .arg("CREATE")
                .arg(dlq_stream)
                .arg(format!("{}-dlq", &config.group_name)) // DLQ group name
                .arg("$") // Start from end
                .arg("MKSTREAM") // Create stream if not exists
                .query_async::<()>(&mut conn)
                .await
                .ok(); // Ignore error if group already exists
        }

        Ok(Self {
            redis,
            config,
            shutdown: Arc::new(Mutex::new(false)),
        })
    }

    /// Create a new backend with default configuration.
    pub async fn with_defaults(redis_url: &str) -> WorkerResult<Self> {
        Self::new(redis_url, RedisStreamConsumerConfig::default()).await
    }

    /// Get the stream name.
    pub fn stream_name(&self) -> &str {
        &self.config.stream_name
    }

    /// Add a message to the stream (for testing/producer usage).
    pub async fn add_message(&self, payload: serde_json::Value) -> WorkerResult<String> {
        let mut conn = self.redis.get().await.map_err(|e| {
            WorkerError::BackendError(format!("Failed to get Redis connection: {}", e))
        })?;

        let message_id: String =
            redis::cmd("XADD")
                .arg(&self.config.stream_name)
                .arg("*")
                .arg("data")
                .arg(serde_json::to_string(&payload).map_err(|e| {
                    WorkerError::BackendError(format!("Failed to serialize: {}", e))
                })?)
                .query_async(&mut conn)
                .await
                .map_err(|e| WorkerError::BackendError(format!("Failed to add message: {}", e)))?;

        Ok(message_id)
    }

    /// Trim the stream to a maximum length using XTRIM.
    ///
    /// This prevents unbounded memory growth in Redis by removing old messages.
    /// Should be called periodically or after processing batches of messages.
    ///
    /// # Arguments
    /// * `max_len` - Maximum number of messages to keep in the stream
    ///
    /// # Example
    /// ```rust,no_run
    /// # use foxtive_worker::backends::RedisStreamBackend;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let backend = RedisStreamBackend::new("redis://localhost", Default::default()).await?;
    /// // Keep only the last 10,000 messages
    /// backend.trim_stream(10000).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn trim_stream(&self, max_len: usize) -> WorkerResult<()> {
        let mut conn = self.redis.get().await.map_err(|e| {
            WorkerError::BackendError(format!("Failed to get Redis connection: {}", e))
        })?;

        redis::cmd("XTRIM")
            .arg(&self.config.stream_name)
            .arg("MAXLEN")
            .arg("~") // Approximate trimming for better performance
            .arg(max_len)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| WorkerError::BackendError(format!("Failed to trim stream: {}", e)))?;

        tracing::debug!(
            "Trimmed stream {} to max length {}",
            self.config.stream_name,
            max_len
        );
        Ok(())
    }

    /// Trim the DLQ stream if configured.
    ///
    /// # Arguments
    /// * `max_len` - Maximum number of messages to keep in the DLQ stream
    pub async fn trim_dlq_stream(&self, max_len: usize) -> WorkerResult<()> {
        if let Some(dlq_stream) = &self.config.dlq_stream_name {
            let mut conn = self.redis.get().await.map_err(|e| {
                WorkerError::BackendError(format!("Failed to get Redis connection: {}", e))
            })?;

            redis::cmd("XTRIM")
                .arg(dlq_stream)
                .arg("MAXLEN")
                .arg("~")
                .arg(max_len)
                .query_async::<()>(&mut conn)
                .await
                .map_err(|e| {
                    WorkerError::BackendError(format!("Failed to trim DLQ stream: {}", e))
                })?;

            tracing::debug!(
                "Trimmed DLQ stream {} to max length {}",
                dlq_stream,
                max_len
            );
        }
        Ok(())
    }

    /// Add a message to the DLQ stream.
    pub async fn add_to_dlq(&self, payload: serde_json::Value) -> WorkerResult<String> {
        let dlq_stream_name = self.config.dlq_stream_name.as_ref().ok_or_else(|| {
            WorkerError::BackendError("DLQ stream name not configured.".to_string())
        })?;

        let mut conn = self.redis.get().await.map_err(|e| {
            WorkerError::BackendError(format!("Failed to get Redis connection: {}", e))
        })?;

        let message_id: String =
            redis::cmd("XADD")
                .arg(dlq_stream_name)
                .arg("*")
                .arg("data")
                .arg(serde_json::to_string(&payload).map_err(|e| {
                    WorkerError::BackendError(format!("Failed to serialize: {}", e))
                })?)
                .query_async(&mut conn)
                .await
                .map_err(|e| {
                    WorkerError::BackendError(format!("Failed to add message to DLQ: {}", e))
                })?;

        Ok(message_id)
    }

    /// Get the payload of a message by its ID from the main stream.
    async fn get_message_payload(
        &self,
        message_id: &str,
    ) -> WorkerResult<Option<serde_json::Value>> {
        let mut conn = self.redis.get().await.map_err(|e| {
            WorkerError::BackendError(format!("Failed to get Redis connection: {}", e))
        })?;

        // XRANGE stream_name message_id message_id
        let result: StreamMessages = redis::cmd("XRANGE")
            .arg(&self.config.stream_name)
            .arg(message_id)
            .arg(message_id)
            .query_async(&mut conn)
            .await
            .map_err(|e| {
                WorkerError::BackendError(format!("Failed to read message for DLQ: {}", e))
            })?;

        if let Some(stream_messages) = result.first()
            && let Some((_, fields)) = stream_messages.first()
            && let Some((_, data_str)) = fields.iter().find(|(k, _)| k == "data")
        {
            // Parse JSON payload - fail on invalid JSON
            let payload: serde_json::Value = serde_json::from_str(data_str).map_err(|e| {
                tracing::error!(
                    "Failed to deserialize message payload for DLQ: {} (message_id: {})",
                    e,
                    message_id
                );
                WorkerError::SerializationError(e)
            })?;
            Ok(Some(payload))
        } else {
            Ok(None)
        }
    }

    /// Get pending message count for this consumer.
    pub async fn pending_count(&self) -> WorkerResult<usize> {
        let mut conn = self.redis.get().await.map_err(|e| {
            WorkerError::BackendError(format!("Failed to get Redis connection: {}", e))
        })?;

        // XPENDING stream group - returns [total_pending, min_id, max_id, [{consumer, count}]]
        let info: Vec<String> = redis::cmd("XPENDING")
            .arg(&self.config.stream_name)
            .arg(&self.config.group_name)
            .query_async(&mut conn)
            .await
            .map_err(|e| {
                WorkerError::BackendError(format!("Failed to get pending count: {}", e))
            })?;

        if let Some(count_str) = info.first()
            && let Ok(count) = count_str.parse::<usize>()
        {
            return Ok(count);
        }
        Ok(0)
    }

    /// Claim pending messages that have been idle for longer than `min_idle_time_ms`.
    /// This is crucial for handling crashed consumers to ensure message delivery.
    pub async fn claim_pending_messages(
        &self,
        min_idle_time_ms: u64,
        count: usize,
    ) -> WorkerResult<Vec<ReceivedMessage<serde_json::Value>>> {
        let mut conn = self.redis.get().await.map_err(|e| {
            WorkerError::BackendError(format!("Failed to get Redis connection: {}", e))
        })?;

        // 1. Get pending message IDs using XPENDING
        let pending_info: Vec<String> = redis::cmd("XPENDING")
            .arg(&self.config.stream_name)
            .arg(&self.config.group_name)
            .arg("-") // Start ID
            .arg("+") // End ID
            .arg(count)
            .query_async(&mut conn)
            .await
            .map_err(|e| WorkerError::BackendError(format!("Failed to get pending IDs: {}", e)))?;

        if pending_info.is_empty() {
            return Ok(Vec::new());
        }

        // Extract message IDs from XPENDING response
        // Response format: [[id, consumer, idle_time, delivery_count], ...]
        let mut ids_to_claim = Vec::new();
        for entry in pending_info.chunks(4) {
            if let Some(id) = entry.first() {
                ids_to_claim.push(id.as_str());
            }
        }

        if ids_to_claim.is_empty() {
            return Ok(Vec::new());
        }

        // 2. Build XCLAIM command
        let mut cmd = redis::cmd("XCLAIM");
        cmd.arg(&self.config.stream_name)
            .arg(&self.config.group_name)
            .arg(&self.config.consumer_name)
            .arg(min_idle_time_ms);

        for id in &ids_to_claim {
            cmd.arg(id);
        }

        // 3. Execute XCLAIM
        let claimed_messages: StreamMessages = cmd
            .query_async(&mut conn)
            .await
            .map_err(|e| WorkerError::BackendError(format!("Failed to claim messages: {}", e)))?;

        let mut received_messages = Vec::new();

        for msg_data in claimed_messages {
            if let Some((message_id, fields)) = msg_data.first()
                && let Some((_, data_str)) = fields.iter().find(|(k, _)| k == "data")
            {
                // Parse JSON payload - fail on invalid JSON
                let payload: serde_json::Value = match serde_json::from_str(data_str) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!(
                            "Failed to deserialize claimed message payload: {} (message_id: {})",
                            e,
                            message_id
                        );
                        // Nack the malformed message without requeue
                        let ack_handle_temp = Arc::new(RedisStreamAckHandle {
                            stream_name: self.config.stream_name.clone(),
                            group_name: self.config.group_name.clone(),
                            message_id: message_id.clone(),
                            redis: self.redis.clone(),
                            backend: Arc::new(Self {
                                redis: self.redis.clone(),
                                config: self.config.clone(),
                                shutdown: self.shutdown.clone(),
                            }),
                        });
                        if let Err(nack_err) = ack_handle_temp.nack(false).await {
                            tracing::error!(
                                "Failed to nack malformed claimed message {}: {:?}",
                                message_id,
                                nack_err
                            );
                        }
                        continue; // Skip this message and process next one
                    }
                };

                let metadata = MessageMetadata::new(&self.config.stream_name);
                let message = Message {
                    id: message_id.clone(),
                    payload,
                    metadata,
                };

                let ack_handle = Arc::new(RedisStreamAckHandle {
                    stream_name: self.config.stream_name.clone(),
                    group_name: self.config.group_name.clone(),
                    message_id: message_id.clone(),
                    redis: self.redis.clone(),
                    backend: Arc::new(Self {
                        redis: self.redis.clone(),
                        config: self.config.clone(),
                        shutdown: self.shutdown.clone(),
                    }),
                });

                received_messages.push(ReceivedMessage::new(message, ack_handle));
            }
        }

        Ok(received_messages)
    }
}

#[async_trait]
impl MessageBackend for RedisStreamBackend {
    async fn receive(&self) -> WorkerResult<ReceiveResult<serde_json::Value>> {
        // Check if shutdown
        {
            let shutdown = self.shutdown.lock().await;
            if *shutdown {
                return Ok(ReceiveResult::Shutdown);
            }
        }

        let mut conn = self.redis.get().await.map_err(|e| {
            WorkerError::BackendError(format!("Failed to get Redis connection: {}", e))
        })?;

        // Read messages from stream
        let result: StreamMessages = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(&self.config.group_name)
            .arg(&self.config.consumer_name)
            .arg("COUNT")
            .arg(self.config.count)
            .arg("BLOCK")
            .arg(self.config.block_ms)
            .arg("STREAMS")
            .arg(&self.config.stream_name)
            .arg(">") // New messages only
            .query_async(&mut conn)
            .await
            .map_err(|e| WorkerError::BackendError(format!("Failed to read from stream: {}", e)))?;

        // Extract first message if available
        if let Some(stream_messages) = result.first()
            && let Some((message_id, fields)) = stream_messages.first()
            && let Some((_, data_str)) = fields.iter().find(|(k, _)| k == "data")
        {
            // Parse JSON payload - fail on invalid JSON
            let payload: serde_json::Value = match serde_json::from_str(data_str) {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!(
                        "Failed to deserialize message payload: {} (message_id: {}, data length: {})",
                        e,
                        message_id,
                        data_str.len()
                    );
                    // Nack the malformed message without requeue to prevent poison pill
                    let ack_handle_temp = Arc::new(RedisStreamAckHandle {
                        stream_name: self.config.stream_name.clone(),
                        group_name: self.config.group_name.clone(),
                        message_id: message_id.clone(),
                        redis: self.redis.clone(),
                        backend: Arc::new(Self {
                            redis: self.redis.clone(),
                            config: self.config.clone(),
                            shutdown: self.shutdown.clone(),
                        }),
                    });
                    if let Err(nack_err) = ack_handle_temp.nack(false).await {
                        tracing::error!(
                            "Failed to nack malformed message {}: {:?}",
                            message_id,
                            nack_err
                        );
                    }
                    // Return Shutdown to skip this message and try to receive the next one
                    return Ok(ReceiveResult::Shutdown);
                }
            };

            // Build metadata
            let metadata = MessageMetadata::new(&self.config.stream_name);
            // Note: stream_id, group, consumer could be added to MessageMetadata if needed

            // Create message
            let message = Message {
                id: message_id.clone(),
                payload,
                metadata,
            };

            // Create ack handle
            let ack_handle = Arc::new(RedisStreamAckHandle {
                stream_name: self.config.stream_name.clone(),
                group_name: self.config.group_name.clone(),
                message_id: message_id.clone(),
                redis: self.redis.clone(),
                backend: Arc::new(Self {
                    redis: self.redis.clone(),
                    config: self.config.clone(),
                    shutdown: self.shutdown.clone(),
                }),
            });

            return Ok(ReceiveResult::Message(ReceivedMessage::new(
                message, ack_handle,
            )));
        }

        // No messages (timeout or empty)
        Ok(ReceiveResult::Timeout)
    }

    async fn ack(&self, message_id: &str) -> WorkerResult<()> {
        let mut conn = self.redis.get().await.map_err(|e| {
            WorkerError::BackendError(format!("Failed to get Redis connection: {}", e))
        })?;

        redis::cmd("XACK")
            .arg(&self.config.stream_name)
            .arg(&self.config.group_name)
            .arg(message_id)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| WorkerError::BackendError(format!("Failed to ack message: {}", e)))?;

        Ok(())
    }

    async fn nack(&self, message_id: &str, requeue: bool) -> WorkerResult<()> {
        if !requeue {
            // If not requeue, move to DLQ and then ACK from main stream
            match self.get_message_payload(message_id).await {
                Ok(Some(payload)) => {
                    if let Err(dlq_err) = self.add_to_dlq(payload).await {
                        tracing::error!(
                            "Failed to add message {} to DLQ via backend nack: {} - will ACK without DLQ",
                            message_id,
                            dlq_err
                        );
                    } else {
                        tracing::info!(
                            "Message {} moved to DLQ successfully via backend nack",
                            message_id
                        );
                    }
                }
                Ok(None) => {
                    tracing::warn!("Message {} not found for DLQ via backend nack", message_id);
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to retrieve message {} for DLQ via backend nack: {} - will ACK without DLQ",
                        message_id,
                        e
                    );
                }
            }

            // Always ACK from main stream to prevent infinite retry loop
            if let Err(ack_err) = self.ack(message_id).await {
                tracing::error!(
                    "Failed to ACK message {} after DLQ attempt via backend nack: {}",
                    message_id,
                    ack_err
                );
                return Err(ack_err);
            }
        } else {
            // If requeue, message remains in PEL
            tracing::warn!(
                "nack called on Redis Stream message {} with requeue=true via backend - message remains pending",
                message_id
            );
        }
        Ok(())
    }

    async fn health_check(&self) -> WorkerResult<()> {
        let mut conn = self.redis.get().await.map_err(|e| {
            WorkerError::BackendError(format!("Failed to get Redis connection: {}", e))
        })?;

        redis::cmd("PING")
            .query_async::<String>(&mut conn)
            .await
            .map_err(|e| WorkerError::BackendError(format!("Redis health check failed: {}", e)))?;

        Ok(())
    }

    async fn shutdown(&self) -> WorkerResult<()> {
        let mut shutdown = self.shutdown.lock().await;
        *shutdown = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    // Note: These tests require a running Redis instance
    // They are marked with #[ignore] to skip in normal test runs

    use super::*;
    use crate::backends::ReceiveResult;
    use crate::backends::redis_stream::RedisStreamConsumerConfig;

    #[tokio::test]
    #[ignore]
    async fn test_connect_and_health() {
        let backend = RedisStreamBackend::with_defaults("redis://localhost")
            .await
            .unwrap();
        assert!(backend.health_check().await.is_ok());
    }

    #[tokio::test]
    #[ignore]
    async fn test_add_and_receive() {
        let backend = RedisStreamBackend::with_defaults("redis://localhost")
            .await
            .unwrap();

        backend
            .add_message(serde_json::json!({"test": "data"}))
            .await
            .unwrap();

        let result = backend.receive().await.unwrap();
        assert!(result.is_message());
    }

    #[tokio::test]
    #[ignore]
    async fn test_pending_count_and_claim() {
        let backend = RedisStreamBackend::with_defaults("redis://localhost")
            .await
            .unwrap();

        // Add a message
        let id = backend
            .add_message(serde_json::json!({"claim_test": true}))
            .await
            .unwrap();

        // Receive it (this moves it to the Pending Entries List)
        let result = backend.receive().await.unwrap();
        assert!(result.is_message());

        // Check pending count
        let count = backend.pending_count().await.unwrap();
        assert!(count > 0);

        // Claim the message (simulating a recovery scenario)
        let claimed = backend.claim_pending_messages(0, 10).await.unwrap();
        assert!(!claimed.is_empty());
        assert_eq!(claimed[0].message.id, id);
    }

    #[tokio::test]
    #[ignore]
    async fn test_nack_to_dlq() {
        let dlq_stream = "worker_stream_dlq".to_string();
        let config = RedisStreamConsumerConfig {
            stream_name: "worker_stream_dlq_test".to_string(),
            dlq_stream_name: Some(dlq_stream.clone()),
            ..Default::default()
        };
        let backend = RedisStreamBackend::new("redis://localhost", config)
            .await
            .unwrap();

        // Add a message
        let message_id = backend
            .add_message(serde_json::json!({"dlq_test": true}))
            .await
            .unwrap();

        // Receive it
        let result = backend.receive().await.unwrap();
        if let ReceiveResult::Message(received_msg) = result {
            assert_eq!(received_msg.message.id, message_id);

            // Nack without requeue (should go to DLQ)
            received_msg.nack(false).await.unwrap();

            // Verify message is acknowledged from main stream (not in PEL)
            let pending_count = backend.pending_count().await.unwrap();
            assert_eq!(pending_count, 0);

            // Verify message is in DLQ stream
            let mut conn = backend.redis.get().await.unwrap();
            let dlq_messages: StreamMessages = redis::cmd("XREAD")
                .arg("COUNT")
                .arg(1)
                .arg("STREAMS")
                .arg(&dlq_stream)
                .arg("0-0")
                .query_async(&mut conn)
                .await
                .unwrap();

            assert!(!dlq_messages.is_empty());
            assert!(!dlq_messages[0].is_empty());
            let (_, fields) = &dlq_messages[0][0];
            let (_, data_str) = fields.iter().find(|(k, _)| k == "data").unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(data_str).unwrap(),
                serde_json::json!({"dlq_test": true})
            );

            // Clean up DLQ stream for next test run
            redis::cmd("DEL")
                .arg(&dlq_stream)
                .query_async::<()>(&mut conn)
                .await
                .unwrap();
            redis::cmd("DEL")
                .arg(&backend.config.stream_name)
                .query_async::<()>(&mut conn)
                .await
                .unwrap();
        } else {
            panic!("Expected Message variant");
        }
    }
}
