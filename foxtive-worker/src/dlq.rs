use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
}
