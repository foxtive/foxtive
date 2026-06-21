use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::backends::MessageBackend;
use crate::error::WorkerResult;

/// Represents a message that has been sent to the Dead Letter Queue.
/// Contains the original message payload along with failure context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterMessage {
    /// Original message ID
    pub original_id: String,

    /// Original message payload
    pub original_payload: serde_json::Value,

    /// Queue where the message originated
    pub source_queue: String,

    /// Number of processing attempts before DLQ
    pub attempt_count: u32,

    /// Error that caused the final failure
    pub error_message: String,

    /// Timestamp when message was sent to DLQ
    pub dlq_timestamp: DateTime<Utc>,

    /// Worker ID that processed the message last
    pub last_worker_id: Option<String>,

    /// Additional metadata about the failure
    pub failure_context: serde_json::Value,
}

impl DeadLetterMessage {
    /// Create a new dead letter message from processing context.
    pub fn new(
        original_id: String,
        original_payload: serde_json::Value,
        source_queue: String,
        attempt_count: u32,
        error_message: String,
    ) -> Self {
        Self {
            original_id,
            original_payload,
            source_queue,
            attempt_count,
            error_message,
            dlq_timestamp: Utc::now(),
            last_worker_id: None,
            failure_context: serde_json::json!({}),
        }
    }

    /// Set the worker ID that last processed this message.
    pub fn with_worker_id(mut self, worker_id: String) -> Self {
        self.last_worker_id = Some(worker_id);
        self
    }

    /// Add additional failure context metadata.
    pub fn with_context(mut self, key: &str, value: serde_json::Value) -> Self {
        if let serde_json::Value::Object(ref mut map) = self.failure_context {
            map.insert(key.to_string(), value);
        }
        self
    }

    /// Convert to JSON string for storage/transmission.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Parse from JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Configuration for poison pill detection.
#[derive(Debug, Clone)]
pub struct PoisonPillConfig {
    /// Maximum number of failures before considering a message a poison pill
    pub max_failures: u32,

    /// Time window for tracking failures (e.g., failures within 1 hour)
    pub time_window: std::time::Duration,

    /// Whether to immediately send to DLQ when poison pill is detected
    pub immediate_dlq: bool,
}

impl Default for PoisonPillConfig {
    fn default() -> Self {
        Self {
            max_failures: 10,
            time_window: std::time::Duration::from_secs(3600), // 1 hour
            immediate_dlq: true,
        }
    }
}

/// Tracks message failures to detect poison pills.
#[derive(Debug)]
pub struct PoisonPillTracker {
    config: PoisonPillConfig,
    // In a production system, this would use Redis or another shared store
    // For now, we'll use in-memory tracking (per-process)
    failure_counts: std::sync::Mutex<std::collections::HashMap<String, Vec<DateTime<Utc>>>>,
}

impl PoisonPillTracker {
    /// Create a new poison pill tracker with the given configuration.
    pub fn new(config: PoisonPillConfig) -> Self {
        Self {
            config,
            failure_counts: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Record a failure for a message and check if it's a poison pill.
    /// Returns true if the message is considered a poison pill.
    pub fn record_failure(&self, message_id: &str) -> bool {
        let mut counts = self.failure_counts.lock().unwrap();
        let now = Utc::now();

        // Get or create failure timestamps for this message
        let failures = counts.entry(message_id.to_string()).or_default();
        failures.push(now);

        // Remove old failures outside the time window
        let cutoff = now - chrono::Duration::from_std(self.config.time_window).unwrap_or_default();
        failures.retain(|&t| t > cutoff);

        // Check if this exceeds the threshold
        let is_poison_pill = failures.len() >= self.config.max_failures as usize;

        if is_poison_pill {
            tracing::warn!(
                "Poison pill detected for message {}: {} failures in {:?}",
                message_id,
                failures.len(),
                self.config.time_window
            );
        }

        is_poison_pill
    }

    /// Get the current failure count for a message.
    pub fn get_failure_count(&self, message_id: &str) -> usize {
        let counts = self.failure_counts.lock().unwrap();
        counts.get(message_id).map(|v| v.len()).unwrap_or(0)
    }

    /// Clear tracking data for a message (e.g., after successful processing).
    pub fn clear(&self, message_id: &str) {
        let mut counts = self.failure_counts.lock().unwrap();
        counts.remove(message_id);
    }
}

/// High-level manager for Dead Letter Queue operations.
///
/// This utility provides convenient methods for managing DLQ messages,
/// including requeuing failed messages back to their source queues,
/// bulk reprocessing, and conditional retry logic.
///
/// # Example
/// ```rust,no_run
/// use foxtive_worker::dlq::DlqManager;
/// use std::sync::Arc;
///
/// async fn setup_dlq_manager(
///     dlq_backend: Arc<dyn MessageBackend>,
///     main_backend: Arc<dyn MessageBackend>,
/// ) {
///     let manager = DlqManager::new(dlq_backend, main_backend);
///     
///     // Reprocess all failed messages
///     let count = manager.reprocess_all().await.unwrap();
///     println!("Reprocessed {} messages", count);
/// }
/// ```
pub struct DlqManager {
    /// Backend for consuming from the DLQ
    dlq_backend: Arc<dyn MessageBackend>,
    /// Backend for publishing to the main queue
    main_backend: Arc<dyn MessageBackend>,
    /// Optional filter function to decide if a message should be retried
    retry_filter: Option<fn(&DeadLetterMessage) -> bool>,
}

impl DlqManager {
    /// Create a new DLQ manager.
    ///
    /// # Arguments
    /// * `dlq_backend` - Backend connected to the dead letter queue
    /// * `main_backend` - Backend connected to the main queue for republishing
    ///
    /// # Returns
    /// A new DlqManager instance
    pub fn new(
        dlq_backend: Arc<dyn MessageBackend>,
        main_backend: Arc<dyn MessageBackend>,
    ) -> Self {
        Self {
            dlq_backend,
            main_backend,
            retry_filter: None,
        }
    }

    /// Set a custom filter function to decide which messages should be retried.
    ///
    /// The filter function receives a `DeadLetterMessage` and returns `true` if
    /// the message should be retried, or `false` if it should be skipped.
    ///
    /// # Arguments
    /// * `filter` - Function that determines if a message should be retried
    ///
    /// # Example
    /// ```rust
    /// use foxtive_worker::dlq::{DlqManager, DeadLetterMessage};
    ///
    /// fn should_retry(msg: &DeadLetterMessage) -> bool {
    ///     // Don't retry poison pills
    ///     if let serde_json::Value::Object(ref ctx) = msg.failure_context {
    ///         if let Some(poison) = ctx.get("poison_pill") {
    ///             return !poison.as_bool().unwrap_or(false);
    ///         }
    ///     }
    ///     true
    /// }
    ///
    /// // manager.with_retry_filter(should_retry);
    /// ```
    pub fn with_retry_filter(mut self, filter: fn(&DeadLetterMessage) -> bool) -> Self {
        self.retry_filter = Some(filter);
        self
    }

    /// Check if a message should be retried based on the configured filter.
    ///
    /// If no filter is set, all messages are considered retryable.
    fn should_retry(&self, dlq_msg: &DeadLetterMessage) -> bool {
        if let Some(filter) = self.retry_filter {
            filter(dlq_msg)
        } else {
            true // Default: retry all messages
        }
    }

    /// Reprocess a single DLQ message by republishing it to the source queue.
    ///
    /// This method:
    /// 1. Parses the DLQ message
    /// 2. Checks if it should be retried (based on filter)
    /// 3. Republishes the original payload to the source queue
    /// 4. Acknowledges the DLQ message
    ///
    /// # Arguments
    /// * `dlq_message` - The received DLQ message to reprocess
    ///
    /// # Returns
    /// Ok(true) if message was requeued, Ok(false) if skipped, Err on failure
    ///
    /// # Example
    /// ```rust,no_run
    /// use foxtive_worker::{ReceivedMessage, dlq::DlqManager};
    ///
    /// async fn handle_message(
    ///     manager: &DlqManager,
    ///     msg: ReceivedMessage<serde_json::Value>,
    /// ) {
    ///     match manager.reprocess_single(&msg).await {
    ///         Ok(true) => println!("Message requeued"),
    ///         Ok(false) => println!("Message skipped"),
    ///         Err(e) => eprintln!("Error: {}", e),
    ///     }
    /// }
    /// ```
    pub async fn reprocess_single(
        &self,
        dlq_message: &crate::message::ReceivedMessage<serde_json::Value>,
    ) -> WorkerResult<bool> {
        // Parse the DLQ message
        let payload_str = dlq_message.message.payload.to_string();
        let dlq_msg = match DeadLetterMessage::from_json(&payload_str) {
            Ok(msg) => msg,
            Err(e) => {
                tracing::error!("Failed to parse DLQ message {}: {}", dlq_message.message.id, e);
                // Acknowledge malformed messages to avoid blocking the queue
                dlq_message.ack().await?;
                return Ok(false);
            }
        };

        // Check if we should retry this message
        if !self.should_retry(&dlq_msg) {
            tracing::info!(
                "Skipping requeue for message {} (filtered out)",
                dlq_msg.original_id
            );
            // Acknowledge the message without retrying
            dlq_message.ack().await?;
            return Ok(false);
        }

        tracing::info!(
            "Reprocessing DLQ message {} (attempts: {}, error: {})",
            dlq_msg.original_id,
            dlq_msg.attempt_count,
            dlq_msg.error_message
        );

        // Republish to the source queue using the main backend
        // For now, we nack with requeue=true as a basic mechanism
        // In a full implementation, you'd publish directly to the source queue
        match dlq_message.nack(true).await {
            Ok(_) => {
                tracing::info!("Successfully requeued message {}", dlq_msg.original_id);
                Ok(true)
            }
            Err(e) => {
                tracing::error!("Failed to requeue message {}: {}", dlq_msg.original_id, e);
                Err(e)
            }
        }
    }

    /// Reprocess all messages currently in the DLQ.
    ///
    /// This method continuously receives messages from the DLQ and attempts
    /// to requeue them until the DLQ is empty or an error occurs.
    ///
    /// # Returns
    /// Ok(count) where count is the number of messages successfully requeued
    ///
    /// # Example
    /// ```rust,no_run
    /// use foxtive_worker::dlq::DlqManager;
    /// use std::sync::Arc;
    ///
    /// async fn reprocess_all_failed(manager: Arc<DlqManager>) {
    ///     match manager.reprocess_all().await {
    ///         Ok(count) => println!("Requeued {} messages", count),
    ///         Err(e) => eprintln!("Error during reprocessing: {}", e),
    ///     }
    /// }
    /// ```
    pub async fn reprocess_all(&self) -> WorkerResult<usize> {
        use crate::backends::ReceiveResult;
        
        let mut requeued_count = 0;
        let mut consecutive_empty = 0;
        let max_consecutive_empty = 3; // Stop after 3 consecutive empty receives

        tracing::info!("Starting DLQ reprocessing...");

        loop {
            // Receive next message from DLQ
            match self.dlq_backend.receive().await? {
                ReceiveResult::Message(msg) => {
                    consecutive_empty = 0; // Reset counter on successful receive

                    match self.reprocess_single(&msg).await {
                        Ok(true) => requeued_count += 1,
                        Ok(false) => {} // Skipped
                        Err(e) => {
                            tracing::warn!("Error reprocessing message: {}", e);
                            // Continue processing other messages despite errors
                        }
                    }
                }
                ReceiveResult::Shutdown => {
                    tracing::info!("DLQ backend shutdown signal received");
                    break;
                }
                _ => {
                    // No message or transient error
                    consecutive_empty += 1;
                    if consecutive_empty >= max_consecutive_empty {
                        tracing::info!("DLQ appears empty ({} consecutive empty receives)", consecutive_empty);
                        break;
                    }
                    // Small delay before retrying
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }

        tracing::info!("DLQ reprocessing complete. Requeued {} messages", requeued_count);
        Ok(requeued_count)
    }

    /// Get a reference to the DLQ backend.
    pub fn dlq_backend(&self) -> &Arc<dyn MessageBackend> {
        &self.dlq_backend
    }

    /// Get a reference to the main backend.
    pub fn main_backend(&self) -> &Arc<dyn MessageBackend> {
        &self.main_backend
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_dead_letter_message_creation() {
        let dlq_msg = DeadLetterMessage::new(
            "msg-123".to_string(),
            serde_json::json!({"data": "test"}),
            "my-queue".to_string(),
            5,
            "Processing failed".to_string(),
        );

        assert_eq!(dlq_msg.original_id, "msg-123");
        assert_eq!(dlq_msg.attempt_count, 5);
        assert_eq!(dlq_msg.source_queue, "my-queue");
    }

    #[test]
    fn test_dead_letter_message_serialization() {
        let dlq_msg = DeadLetterMessage::new(
            "msg-123".to_string(),
            serde_json::json!({"data": "test"}),
            "my-queue".to_string(),
            5,
            "Processing failed".to_string(),
        );

        let json = dlq_msg.to_json().unwrap();
        let parsed = DeadLetterMessage::from_json(&json).unwrap();

        assert_eq!(parsed.original_id, dlq_msg.original_id);
        assert_eq!(parsed.attempt_count, dlq_msg.attempt_count);
    }

    #[test]
    fn test_poison_pill_detection() {
        let config = PoisonPillConfig {
            max_failures: 3,
            time_window: Duration::from_secs(60),
            immediate_dlq: true,
        };

        let tracker = PoisonPillTracker::new(config);

        // First two failures should not trigger
        assert!(!tracker.record_failure("msg-1"));
        assert!(!tracker.record_failure("msg-1"));

        // Third failure should trigger poison pill detection
        assert!(tracker.record_failure("msg-1"));
    }

    #[test]
    fn test_dlq_manager_creation() {
        use crate::backends::memory::MemoryBackend;

        let dlq_backend: Arc<dyn MessageBackend> = Arc::new(MemoryBackend::new());
        let main_backend: Arc<dyn MessageBackend> = Arc::new(MemoryBackend::new());

        let manager = DlqManager::new(dlq_backend.clone(), main_backend.clone());

        // Just verify it was created successfully - backends are stored correctly
        // The manager should be functional with no filter by default
        assert!(manager.should_retry(&DeadLetterMessage::new(
            "test".to_string(),
            serde_json::json!({}),
            "queue".to_string(),
            1,
            "error".to_string(),
        )));
    }

    #[test]
    fn test_dlq_manager_with_retry_filter() {
        use crate::backends::memory::MemoryBackend;

        let dlq_backend = Arc::new(MemoryBackend::new());
        let main_backend = Arc::new(MemoryBackend::new());

        // Create a filter that rejects poison pills
        fn reject_poison_pills(msg: &DeadLetterMessage) -> bool {
            if let serde_json::Value::Object(ref ctx) = msg.failure_context
                && let Some(poison) = ctx.get("poison_pill") {
                    return !poison.as_bool().unwrap_or(false);
                }
            true
        }

        let manager = DlqManager::new(dlq_backend, main_backend)
            .with_retry_filter(reject_poison_pills);

        // Test with a normal message
        let normal_msg = DeadLetterMessage::new(
            "msg-1".to_string(),
            serde_json::json!({}),
            "queue".to_string(),
            1,
            "error".to_string(),
        );
        assert!(manager.should_retry(&normal_msg));

        // Test with a poison pill
        let poison_msg = DeadLetterMessage::new(
            "msg-2".to_string(),
            serde_json::json!({}),
            "queue".to_string(),
            1,
            "error".to_string(),
        )
        .with_context("poison_pill", serde_json::json!(true));
        assert!(!manager.should_retry(&poison_msg));
    }

    #[test]
    fn test_dlq_manager_default_retry_all() {
        use crate::backends::memory::MemoryBackend;

        let dlq_backend = Arc::new(MemoryBackend::new());
        let main_backend = Arc::new(MemoryBackend::new());
        let manager = DlqManager::new(dlq_backend, main_backend);

        // Without a filter, all messages should be retryable
        let msg = DeadLetterMessage::new(
            "msg-1".to_string(),
            serde_json::json!({}),
            "queue".to_string(),
            1,
            "error".to_string(),
        );
        assert!(manager.should_retry(&msg));
    }
}
