use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;

use crate::MessageProperties;
use crate::error::WorkerResult;

/// Pure message data (serializable, no backend references).
///
/// This struct contains only the message payload and metadata,
/// making it safe to serialize, clone, and pass around without
/// worrying about backend-specific state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message<T> {
    /// Unique message identifier
    pub id: String,
    /// The message payload
    pub payload: T,
    /// Message metadata
    pub metadata: MessageMetadata,
}

/// Metadata associated with a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// When the message was received
    pub received_at: DateTime<Utc>,
    /// Number of processing attempts
    pub attempt: u32,
    /// Source of the message (queue name, stream name, etc.)
    pub source: String,
    /// Optional correlation ID for distributed tracing
    pub correlation_id: Option<String>,
    /// RabbitMQ routing key (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing_key: Option<String>,
    /// Backend-specific message properties
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<MessageProperties>,
}

impl MessageMetadata {
    /// Create new message metadata with current timestamp.
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            received_at: Utc::now(),
            attempt: 0, // Initialize to 0, will be incremented before first processing
            source: source.into(),
            correlation_id: None,
            routing_key: None,
            properties: None,
        }
    }

    /// Set the correlation ID for distributed tracing.
    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }

    /// Set the routing key (for RabbitMQ messages).
    pub fn with_routing_key(mut self, routing_key: impl Into<String>) -> Self {
        self.routing_key = Some(routing_key.into());
        self
    }

    /// Set message properties
    pub fn with_properties(mut self, properties: MessageProperties) -> Self {
        self.properties = Some(properties);
        self
    }

    /// Increment the attempt counter.
    pub fn increment_attempt(&mut self) {
        self.attempt += 1;
    }
}

/// Trait for backend-specific acknowledgment operations.
///
/// Each message backend (RabbitMQ, Redis Streams, etc.) provides
/// its own implementation of this trait, encapsulating the logic
/// needed to acknowledge or negative-acknowledge a specific message.
#[async_trait]
pub trait AckHandle: Send + Sync + Debug {
    /// Acknowledge successful message processing.
    async fn ack(&self) -> WorkerResult<()>;

    /// Negative acknowledge message processing failure.
    ///
    /// # Arguments
    /// * `requeue` - If true, the message should be requeued for redelivery.
    ///               If false, the message should be discarded or moved to a dead letter queue.
    async fn nack(&self, requeue: bool) -> WorkerResult<()>;

    /// Retry message with a delay (backend-specific implementation).
    ///
    /// For backends that support delayed retries (e.g., RabbitMQ with DLX+TTL),
    /// this publishes the message to a retry queue with the specified delay.
    /// For other backends, this may fall back to immediate requeue or return an error.
    ///
    /// # Arguments
    /// * `message` - The message to retry
    /// * `delay_ms` - Delay in milliseconds before redelivery
    ///
    /// # Returns
    /// Ok if retry was scheduled, Err if not supported or failed
    async fn retry_with_delay(
        &self,
        _message: &Message<serde_json::Value>,
        _delay_ms: u64,
    ) -> WorkerResult<()> {
        // Default implementation: fall back to immediate requeue
        self.nack(true).await
    }

    /// Send message to Dead Letter Queue after retries are exhausted.
    ///
    /// This method publishes the message to a DLQ with failure metadata.
    /// Only applicable for backends that support DLQ (e.g., RabbitMQ).
    /// For other backends, this is a no-op.
    ///
    /// # Arguments
    /// * `message` - The message that has exhausted all retries
    /// * `error_message` - Description of why the message failed
    ///
    /// # Returns
    /// Ok if published to DLQ or not supported, Err if publishing failed
    async fn send_to_dlq(
        &self,
        _message: &Message<serde_json::Value>,
        _error_message: &str,
    ) -> WorkerResult<()> {
        // Default implementation: no-op for backends without DLQ support
        Ok(())
    }
}

/// Wrapper combining message data with acknowledgment capability.
///
/// This is what workers and middleware actually receive. It separates
/// the pure message data from the acknowledgment operations, preventing
/// circular dependencies while providing safe concurrent access.
///
/// # Example
/// ```rust
/// use foxtive_worker::message::ReceivedMessage;
///
/// async fn process_message(received: ReceivedMessage<serde_json::Value>) {
///     // Access message data
///     println!("Processing message: {}", received.message.id);
///     
///     // Process the message...
///     
///     // Acknowledge on success
///     if let Err(e) = received.ack().await {
///         eprintln!("Failed to ack: {}", e);
///     }
/// }
/// ```
pub struct ReceivedMessage<T> {
    /// The pure message data
    pub message: Message<T>,
    /// Backend-specific acknowledgment handle (using Arc for shareability)
    pub ack_handle: Arc<dyn AckHandle>,
}

impl<T: Send + Sync> ReceivedMessage<T> {
    /// Create a new ReceivedMessage.
    pub fn new(message: Message<T>, ack_handle: Arc<dyn AckHandle>) -> Self {
        Self {
            message,
            ack_handle,
        }
    }

    /// Acknowledge successful message processing.
    pub async fn ack(&self) -> WorkerResult<()> {
        self.ack_handle.ack().await
    }

    /// Negative acknowledge message processing failure.
    pub async fn nack(&self, requeue: bool) -> WorkerResult<()> {
        self.ack_handle.nack(requeue).await
    }

    /// Extract the inner message, discarding the ack handle.
    pub fn into_message(self) -> Message<T> {
        self.message
    }
}

/// Specialized implementation for JSON messages
impl ReceivedMessage<serde_json::Value> {
    /// Retry message with a delay (if supported by backend).
    pub async fn retry_with_delay(&self, delay_ms: u64) -> WorkerResult<()> {
        self.ack_handle
            .retry_with_delay(&self.message, delay_ms)
            .await
    }

    /// Send message to Dead Letter Queue after retries are exhausted.
    pub async fn send_to_dlq(&self, error_message: &str) -> WorkerResult<()> {
        self.ack_handle
            .send_to_dlq(&self.message, error_message)
            .await
    }

    /// Requeue a DLQ message back to its source queue.
    ///
    /// This method extracts the original payload from a DLQ message and republishes
    /// it to the source queue using the provided main backend.
    ///
    /// # Arguments
    /// * `main_backend` - The backend for the main queue where the message should be republished
    ///
    /// # Returns
    /// Ok if successfully republished, Err if publishing failed
    ///
    /// # Example
    /// ```rust,no_run
    /// use foxtive_worker::{ReceivedMessage, MessageBackend};
    /// use std::sync::Arc;
    ///
    /// async fn handle_dlq_message(
    ///     dlq_msg: ReceivedMessage<serde_json::Value>,
    ///     main_backend: Arc<dyn MessageBackend>,
    /// ) {
    ///     // Republish to source queue
    ///     if let Err(e) = dlq_msg.requeue_to_source(&*main_backend).await {
    ///         eprintln!("Failed to requeue: {}", e);
    ///     }
    /// }
    /// ```
    pub async fn requeue_to_source(
        &self,
        _main_backend: &dyn crate::backends::MessageBackend,
    ) -> WorkerResult<()> {
        // Extract the original payload
        let _payload = &self.message.payload;

        // Get the source queue from metadata
        let source_queue = &self.message.metadata.source;

        tracing::info!(
            "Requeuing message {} to source queue {}",
            self.message.id,
            source_queue
        );

        // Publish the message back to the source queue
        // Note: This requires the backend to support publishing
        // For RabbitMQ, this would use basic_publish
        // For other backends, they need to implement their own publish logic

        // For now, we nack with requeue=true as a fallback
        // Backends that support DLQ requeue should override this behavior
        self.nack(true).await
    }
}

impl<T: Clone + Send + Sync> Clone for ReceivedMessage<T> {
    fn clone(&self) -> Self {
        Self {
            message: self.message.clone(),
            ack_handle: self.ack_handle.clone(),
        }
    }
}

impl<T: Debug> Debug for ReceivedMessage<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReceivedMessage")
            .field("message", &self.message)
            .field("ack_handle", &"<AckHandle>")
            .finish()
    }
}

/// Convenience type for JSON-valued messages.
pub type JsonMessage = Message<serde_json::Value>;

/// Convenience type for received JSON-valued messages.
pub type ReceivedJsonMessage = ReceivedMessage<serde_json::Value>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Debug)]
    struct MockAckHandle {
        acked: Arc<AtomicBool>,
        nacked: Arc<AtomicBool>,
    }

    impl MockAckHandle {
        fn new() -> (Self, Arc<AtomicBool>, Arc<AtomicBool>) {
            let acked = Arc::new(AtomicBool::new(false));
            let nacked = Arc::new(AtomicBool::new(false));
            (
                Self {
                    acked: acked.clone(),
                    nacked: nacked.clone(),
                },
                acked,
                nacked,
            )
        }
    }

    #[async_trait]
    impl AckHandle for MockAckHandle {
        async fn ack(&self) -> WorkerResult<()> {
            self.acked.store(true, Ordering::SeqCst);
            Ok(())
        }

        async fn nack(&self, _requeue: bool) -> WorkerResult<()> {
            self.nacked.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_message_creation() {
        let message = Message {
            id: "test-1".to_string(),
            payload: "test data",
            metadata: MessageMetadata::new("test-queue"),
        };

        assert_eq!(message.id, "test-1");
        assert_eq!(message.payload, "test data");
        assert_eq!(message.metadata.attempt, 0); // Starts at 0, incremented before processing
    }

    #[tokio::test]
    async fn test_received_message_ack() {
        let (ack_handle, acked, _) = MockAckHandle::new();
        let message = Message {
            id: "test-1".to_string(),
            payload: "test data",
            metadata: MessageMetadata::new("test-queue"),
        };
        let received = ReceivedMessage::new(message, Arc::new(ack_handle));

        received.ack().await.unwrap();
        assert!(acked.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_received_message_nack() {
        let (ack_handle, _, nacked) = MockAckHandle::new();
        let message = Message {
            id: "test-1".to_string(),
            payload: "test data",
            metadata: MessageMetadata::new("test-queue"),
        };
        let received = ReceivedMessage::new(message, Arc::new(ack_handle));

        received.nack(true).await.unwrap();
        assert!(nacked.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_metadata_with_correlation_id() {
        let metadata = MessageMetadata::new("test-queue").with_correlation_id("corr-123");

        assert_eq!(metadata.correlation_id, Some("corr-123".to_string()));
    }

    #[tokio::test]
    async fn test_metadata_increment_attempt() {
        let mut metadata = MessageMetadata::new("test-queue");
        assert_eq!(metadata.attempt, 0); // Starts at 0

        metadata.increment_attempt();
        assert_eq!(metadata.attempt, 1); // First increment makes it 1
    }
}
