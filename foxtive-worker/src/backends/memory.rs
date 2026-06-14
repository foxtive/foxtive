use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

use crate::backends::contract::MessageBackend;
use crate::backends::ReceiveResult;
use crate::error::WorkerResult;
use crate::message::{AckHandle, Message, MessageMetadata, ReceivedMessage};
use crate::MessageProperties;

/// In-memory acknowledgment handle.
#[derive(Debug)]
pub struct MemoryAckHandle {
    message_id: String,
    backend: Arc<MemoryBackendInner>,
}

#[async_trait]
impl AckHandle for MemoryAckHandle {
    async fn ack(&self) -> WorkerResult<()> {
        self.backend.ack(&self.message_id)
    }

    async fn nack(&self, requeue: bool) -> WorkerResult<()> {
        self.backend.nack(&self.message_id, requeue)
    }
}

/// Internal state for the memory backend.
#[derive(Debug)]
struct MemoryBackendInner {
    queue: Mutex<VecDeque<Message<serde_json::Value>>>,
    unacked: Mutex<std::collections::HashMap<String, Message<serde_json::Value>>>,
    notify: Notify,
    shutdown: Mutex<bool>,
}

impl MemoryBackendInner {
    fn ack(&self, message_id: &str) -> WorkerResult<()> {
        let mut unacked = self.unacked.lock().unwrap();
        unacked.remove(message_id);
        Ok(())
    }

    fn nack(&self, message_id: &str, requeue: bool) -> WorkerResult<()> {
        let mut unacked = self.unacked.lock().unwrap();
        if let Some(message) = unacked.remove(message_id)
            && requeue
        {
            self.queue.lock().unwrap().push_back(message);
            self.notify.notify_one();
        }
        Ok(())
    }
}

/// In-memory message backend for testing and development.
///
/// This backend stores messages in memory and provides a simple queue
/// for testing worker implementations without external dependencies.
///
/// # Example
/// ```rust
/// use foxtive_worker::backends::MemoryBackend;
///
/// let backend = MemoryBackend::new();
/// backend.enqueue(serde_json::json!({"key": "value"}));
/// ```
pub struct MemoryBackend {
    inner: Arc<MemoryBackendInner>,
    source: String,
}

impl std::fmt::Debug for MemoryBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryBackend")
            .field("source", &self.source)
            .finish()
    }
}

impl MemoryBackend {
    /// Create a new in-memory backend with the default source name.
    pub fn new() -> Self {
        Self::with_source("memory-queue")
    }

    /// Create a new in-memory backend with a custom source name.
    pub fn with_source(source: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(MemoryBackendInner {
                queue: Mutex::new(VecDeque::new()),
                unacked: Mutex::new(std::collections::HashMap::new()),
                notify: Notify::new(),
                shutdown: Mutex::new(false),
            }),
            source: source.into(),
        }
    }

    /// Enqueue a message for processing.
    ///
    /// # Arguments
    /// * `payload` - The message payload
    ///
    /// # Returns
    /// The message ID
    pub fn enqueue(&self, payload: serde_json::Value) -> String {
        self.enqueue_with_properties(payload, None)
    }

    /// Enqueue a message with custom properties.
    ///
    /// # Arguments
    /// * `payload` - The message payload
    /// * `properties` - Optional message properties
    ///
    /// # Returns
    /// The message ID
    pub fn enqueue_with_properties(
        &self,
        payload: serde_json::Value,
        properties: Option<MessageProperties>,
    ) -> String {
        let message_id = uuid::Uuid::new_v4().to_string();
        let mut metadata = MessageMetadata::new(&self.source);
        if let Some(props) = properties {
            metadata = metadata.with_properties(props);
        }

        let message = Message {
            id: message_id.clone(),
            payload,
            metadata,
        };

        let mut queue = self.inner.queue.lock().unwrap();
        queue.push_back(message);

        // Notify waiting receivers
        self.inner.notify.notify_one();

        message_id
    }

    /// Enqueue multiple messages.
    pub fn enqueue_batch(&self, payloads: Vec<serde_json::Value>) -> Vec<String> {
        payloads.into_iter().map(|p| self.enqueue(p)).collect()
    }

    /// Get the number of messages currently in the queue.
    pub fn queue_len(&self) -> usize {
        let queue = self.inner.queue.lock().unwrap();
        queue.len()
    }

    /// Get the number of unacknowledged messages.
    pub fn unacked_count(&self) -> usize {
        let unacked = self.inner.unacked.lock().unwrap();
        unacked.len()
    }

    /// Clear all messages from the queue.
    pub fn clear(&self) {
        let mut queue = self.inner.queue.lock().unwrap();
        queue.clear();
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MessageBackend for MemoryBackend {
    async fn receive(&self) -> WorkerResult<ReceiveResult<serde_json::Value>> {
        // Check if shutdown
        {
            let shutdown = self.inner.shutdown.lock().unwrap();
            if *shutdown {
                return Ok(ReceiveResult::Shutdown);
            }
        }

        // Try to get a message from the queue
        loop {
            // Check queue first
            {
                let mut queue = self.inner.queue.lock().unwrap();
                if let Some(message) = queue.pop_front() {
                    let message_id = message.id.clone();

                    // Track as unacked
                    {
                        let mut unacked = self.inner.unacked.lock().unwrap();
                        unacked.insert(message_id.clone(), message.clone());
                    }

                    let ack_handle = Arc::new(MemoryAckHandle {
                        message_id,
                        backend: self.inner.clone(),
                    });

                    return Ok(ReceiveResult::Message(Box::from(ReceivedMessage::new(
                        message, ack_handle,
                    ))));
                }
            }

            // Check shutdown again
            {
                let shutdown = self.inner.shutdown.lock().unwrap();
                if *shutdown {
                    return Ok(ReceiveResult::Shutdown);
                }
            }

            // Wait for notification
            self.inner.notify.notified().await;
        }
    }

    async fn ack(&self, message_id: &str) -> WorkerResult<()> {
        self.inner.ack(message_id)
    }

    async fn nack(&self, message_id: &str, requeue: bool) -> WorkerResult<()> {
        self.inner.nack(message_id, requeue)
    }

    async fn health_check(&self) -> WorkerResult<()> {
        // Memory backend is always healthy
        Ok(())
    }

    async fn shutdown(&self) -> WorkerResult<()> {
        let mut shutdown = self.inner.shutdown.lock().unwrap();
        *shutdown = true;
        // Wake up any waiting receivers
        self.inner.notify.notify_waiters();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::ReceiveResult;

    #[tokio::test]
    async fn test_enqueue_and_receive() {
        let backend = MemoryBackend::new();

        backend.enqueue(serde_json::json!({"test": "data"}));

        let result = backend.receive().await.unwrap();
        assert!(result.is_message());

        if let ReceiveResult::Message(message) = result {
            assert_eq!(message.message.payload["test"], "data");
        } else {
            panic!("Expected Message variant");
        }
    }

    #[tokio::test]
    async fn test_ack_removes_from_unacked() {
        let backend = MemoryBackend::new();

        backend.enqueue(serde_json::json!({"test": "data"}));

        let result = backend.receive().await.unwrap();
        if let ReceiveResult::Message(received) = result {
            assert_eq!(backend.unacked_count(), 1);
            received.ack().await.unwrap();
            assert_eq!(backend.unacked_count(), 0);
        } else {
            panic!("Expected Message variant");
        }
    }

    #[tokio::test]
    async fn test_nack_with_requeue() {
        let backend = MemoryBackend::new();

        backend.enqueue(serde_json::json!({"test": "data"}));

        let result = backend.receive().await.unwrap();
        if let ReceiveResult::Message(received) = result {
            assert_eq!(backend.queue_len(), 0);
            received.nack(true).await.unwrap();
            assert_eq!(backend.queue_len(), 1); // Requeued
        } else {
            panic!("Expected Message variant");
        }
    }

    #[tokio::test]
    async fn test_nack_without_requeue() {
        let backend = MemoryBackend::new();

        backend.enqueue(serde_json::json!({"test": "data"}));

        let result = backend.receive().await.unwrap();
        if let ReceiveResult::Message(received) = result {
            received.nack(false).await.unwrap();
            assert_eq!(backend.queue_len(), 0); // Not requeued
            assert_eq!(backend.unacked_count(), 0); // Removed from unacked
        } else {
            panic!("Expected Message variant");
        }
    }

    #[tokio::test]
    async fn test_shutdown() {
        let backend = MemoryBackend::new();

        backend.shutdown().await.unwrap();

        let result = backend.receive().await.unwrap();
        assert!(result.is_shutdown());
    }

    #[tokio::test]
    async fn test_health_check() {
        let backend = MemoryBackend::new();
        assert!(backend.health_check().await.is_ok());
    }

    #[tokio::test]
    async fn test_queue_len() {
        let backend = MemoryBackend::new();

        backend.enqueue(serde_json::json!({"msg": 1}));
        backend.enqueue(serde_json::json!({"msg": 2}));
        backend.enqueue(serde_json::json!({"msg": 3}));

        assert_eq!(backend.queue_len(), 3);
    }

    #[tokio::test]
    async fn test_clear() {
        let backend = MemoryBackend::new();

        backend.enqueue(serde_json::json!({"msg": 1}));
        backend.enqueue(serde_json::json!({"msg": 2}));

        backend.clear();
        assert_eq!(backend.queue_len(), 0);
    }
}
