use async_trait::async_trait;
use std::time::Duration;

use crate::error::WorkerResult;
use crate::message::ReceivedMessage;

/// Result of a receive operation from a message backend.
///
/// This enum provides explicit semantics for different receive outcomes,
/// allowing callers to distinguish between graceful shutdown, connection
/// loss, transient errors, and successful message receipt.
///
/// # Examples
/// ```no_run
/// match backend.receive_with_status().await? {
///     ReceiveResult::Message(msg) => {
///         // Process the message
///         println!("Received: {}", msg.message.id);
///     }
///     ReceiveResult::Shutdown => {
///         // Backend is shutting down gracefully
///         break;
///     }
///     ReceiveResult::ConnectionLost { reason } => {
///         // Connection lost - trigger reconnection
///         eprintln!("Connection lost: {}", reason);
///         return Err(anyhow::anyhow!("Connection lost: {}", reason));
///     }
///     _ => {
///         // Handle other cases
///     }
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum ReceiveResult<T> {
    /// Message received successfully
    #[error("Message received")]
    Message(ReceivedMessage<T>),
    
    /// Backend is shutting down gracefully (shutdown() was called)
    #[error("Backend shutting down")]
    Shutdown,
    
    /// Connection lost - requires reconnection
    #[error("Connection lost: {reason}")]
    ConnectionLost { 
        /// Reason for connection loss
        reason: String 
    },
    
    /// Operation timed out
    #[error("Operation timed out")]
    Timeout,
    
    /// Consumer channel was closed unexpectedly
    #[error("Consumer channel closed")]
    ChannelClosed,
    
    /// Consumer was cancelled by broker or client
    #[error("Consumer cancelled")]
    ConsumerCancelled,
    
    /// Transient error that may resolve on retry
    #[error("Retryable error: {reason}")]
    Retryable { 
        /// Error description
        reason: String, 
        /// Optional suggested wait time before retrying
        retry_after: Option<Duration> 
    },
}

impl<T> ReceiveResult<T> {
    /// Check if a message was received
    pub fn is_message(&self) -> bool {
        matches!(self, ReceiveResult::Message(_))
    }

    /// Check if the backend is shutting down
    pub fn is_shutdown(&self) -> bool {
        matches!(self, ReceiveResult::Shutdown)
    }

    /// Check if reconnection is needed
    pub fn needs_reconnection(&self) -> bool {
        matches!(self, ReceiveResult::ConnectionLost { .. })
    }

    /// Check if the operation should be retried
    pub fn is_retryable(&self) -> bool {
        matches!(self, ReceiveResult::Retryable { .. })
    }

    /// Extract the message if present
    pub fn into_message(self) -> Option<ReceivedMessage<T>> {
        match self {
            ReceiveResult::Message(msg) => Some(msg),
            _ => None,
        }
    }

    /// Get a description of the result status
    pub fn status_str(&self) -> &'static str {
        match self {
            ReceiveResult::Message(_) => "message",
            ReceiveResult::Shutdown => "shutdown",
            ReceiveResult::ConnectionLost { .. } => "connection_lost",
            ReceiveResult::Timeout => "timeout",
            ReceiveResult::ChannelClosed => "channel_closed",
            ReceiveResult::ConsumerCancelled => "consumer_cancelled",
            ReceiveResult::Retryable { .. } => "retryable",
        }
    }
}

/// Trait for message backend implementations.
///
/// Backends are responsible for receiving messages from external sources
/// (RabbitMQ, Redis Streams, etc.) and providing acknowledgment handles.
#[async_trait]
pub trait MessageBackend: Send + Sync {
    /// Receive the next message with detailed status information.
    ///
    /// This is the primary receive method that provides explicit semantics
    /// for different receive outcomes (shutdown, connection lost, retryable errors, etc.).
    ///
    /// # Returns
    /// * `Ok(ReceiveResult)` - Contains the receive outcome
    /// * `Err(WorkerError)` - Fatal error occurred
    async fn receive(&self) -> WorkerResult<ReceiveResult<serde_json::Value>>;

    /// Acknowledge a message by ID.
    ///
    /// This is used for batch acknowledgments or when the AckHandle is not available.
    /// For single message acks, prefer using the AckHandle on ReceivedMessage.
    ///
    /// # Arguments
    /// * `message_id` - The ID of the message to acknowledge
    async fn ack(&self, message_id: &str) -> WorkerResult<()>;

    /// Negative acknowledge a message by ID.
    ///
    /// # Arguments
    /// * `message_id` - The ID of the message to negative-acknowledge
    /// * `requeue` - If true, requeue the message for redelivery
    async fn nack(&self, message_id: &str, requeue: bool) -> WorkerResult<()>;

    /// Perform a health check on the backend.
    ///
    /// # Returns
    /// * `Ok(())` - Backend is healthy
    /// * `Err(WorkerError)` - Backend is unhealthy
    async fn health_check(&self) -> WorkerResult<()>;

    /// Gracefully shutdown the backend.
    ///
    /// This should stop accepting new messages and clean up resources.
    async fn shutdown(&self) -> WorkerResult<()>;
}
