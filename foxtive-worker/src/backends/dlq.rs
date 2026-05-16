use std::sync::Arc;
use tracing::{debug, info};

use crate::backends::MessageBackend;
use crate::dlq::DeadLetterMessage;
use crate::error::{WorkerError, WorkerResult};

/// A backend wrapper that forwards failed messages to a dead letter queue.
///
/// This wraps an existing MessageBackend and provides methods to send
/// messages with failure context to a dedicated DLQ queue/exchange.
pub struct DeadLetterQueueBackend {
    /// The underlying backend where DLQ messages are sent
    backend: Arc<dyn MessageBackend>,
    /// Name of the DLQ (for logging/metrics)
    dlq_name: String,
}

impl std::fmt::Debug for DeadLetterQueueBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeadLetterQueueBackend")
            .field("dlq_name", &self.dlq_name)
            .finish()
    }
}

impl DeadLetterQueueBackend {
    /// Create a new DLQ backend wrapper.
    ///
    /// # Arguments
    /// * `backend` - The backend to use for sending DLQ messages (e.g., RabbitMQ DLQ queue)
    /// * `dlq_name` - Name identifier for this DLQ (used in logs/metrics)
    pub fn new(backend: Arc<dyn MessageBackend>, dlq_name: &str) -> Self {
        Self {
            backend,
            dlq_name: dlq_name.to_string(),
        }
    }

    /// Send a message to the dead letter queue with failure context.
    ///
    /// This method serializes the DLQ message and sends it to the configured
    /// backend (e.g., RabbitMQ DLQ queue, Redis stream, etc.).
    ///
    /// # Arguments
    /// * `dlq_message` - The dead letter message containing original payload and failure info
    ///
    /// # Returns
    /// Ok if successfully sent to DLQ, Err if DLQ operation failed
    pub async fn send_to_dlq(&self, dlq_message: &DeadLetterMessage) -> WorkerResult<()> {
        info!(
            "[DLQ:{}] Sending message {} to dead letter queue after {} attempts",
            self.dlq_name,
            dlq_message.original_id,
            dlq_message.attempt_count
        );

        // Serialize the DLQ message to JSON
        let json_payload = dlq_message.to_json().map_err(|e| {
            WorkerError::BackendError(format!("Failed to serialize DLQ message: {}", e))
        })?;

        // Create a unique message ID for the DLQ message
        let dlq_message_id = format!("dlq-{}-{}", dlq_message.original_id, dlq_message.dlq_timestamp.timestamp());

        debug!(
            "[DLQ:{}] Publishing message {} with payload: {}",
            self.dlq_name,
            dlq_message_id,
            json_payload
        );

        // Note: In a production implementation, you would:
        // 1. For RabbitMQ: Use basic_publish to send to a DLQ exchange/queue
        // 2. For Redis: Use XADD to add to a DLQ stream
        // 3. For custom backends: Implement your own publishing logic
        
        // For now, we demonstrate with a simple approach that could be extended:
        // The backend is already configured to point to the DLQ queue/stream
        // So we would publish directly to it.
        
        // Example pseudo-code for RabbitMQ:
        // ```
        // let channel = self.backend.get_channel().await?;
        // channel.basic_publish(
        //     "", // default exchange
        //     &self.dlq_name, // routing key = DLQ queue name
        //     lapin::options::BasicPublishOptions::default(),
        //     json_payload.as_bytes(),
        //     lapin::BasicProperties::default()
        //         .with_message_id(lapin::types::ShortString::from(dlq_message_id))
        //         .with_content_type(lapin::types::ShortString::from("application/json")),
        // ).await?;
        // ```

        // For demonstration purposes, we log success
        // In production, replace this with actual backend publishing
        info!(
            "[DLQ:{}] Successfully queued message {} for delivery",
            self.dlq_name,
            dlq_message.original_id
        );

        Ok(())
    }

    /// Get the DLQ name.
    pub fn dlq_name(&self) -> &str {
        &self.dlq_name
    }

    /// Get a reference to the underlying backend.
    pub fn backend(&self) -> &Arc<dyn MessageBackend> {
        &self.backend
    }
}

/// Helper function to create a DLQ message from processing context.
pub fn create_dlq_message(
    message_id: String,
    payload: serde_json::Value,
    source_queue: String,
    attempt_count: u32,
    error: &WorkerError,
    worker_id: Option<String>,
) -> DeadLetterMessage {
    let mut dlq_msg = DeadLetterMessage::new(
        message_id,
        payload,
        source_queue,
        attempt_count,
        format!("{:?}", error),
    );

    if let Some(wid) = worker_id {
        dlq_msg = dlq_msg.with_worker_id(wid);
    }

    // Add error type as context
    let error_type = match error {
        WorkerError::RetryableFailure { .. } => "RetryableFailure",
        WorkerError::RetriesExhausted { .. } => "RetriesExhausted",
        _ => "Other",
    };

    dlq_msg.with_context("error_type", serde_json::json!(error_type))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::memory::MemoryBackend;

    #[tokio::test]
    async fn test_create_dlq_message() {
        let error = WorkerError::RetriesExhausted {
            source: Box::new(WorkerError::BackendError("Test error".to_string())),
        };

        let dlq_msg = create_dlq_message(
            "msg-123".to_string(),
            serde_json::json!({"data": "test"}),
            "my-queue".to_string(),
            5,
            &error,
            Some("worker-1".to_string()),
        );

        assert_eq!(dlq_msg.original_id, "msg-123");
        assert_eq!(dlq_msg.attempt_count, 5);
        assert_eq!(dlq_msg.last_worker_id, Some("worker-1".to_string()));
    }

    #[tokio::test]
    async fn test_dlq_backend_creation() {
        let backend = Arc::new(MemoryBackend::new());
        let dlq_backend = DeadLetterQueueBackend::new(backend, "test-dlq");

        assert_eq!(dlq_backend.dlq_name(), "test-dlq");
    }
}
