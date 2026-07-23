#![cfg(feature = "rabbitmq")]

use foxtive_worker::backends::rabbitmq::RabbitMqConsumerConfig;
use foxtive_worker::message::{AckHandle, Message, MessageMetadata};
use std::sync::Arc;

/// Test that config defaults are correct
#[test]
fn test_config_defaults() {
    let config = RabbitMqConsumerConfig::default();

    assert_eq!(config.queue_name, "worker_queue");
    assert_eq!(config.consumer_tag, "foxtive-worker");
    assert!(!config.auto_ack);
    assert_eq!(config.prefetch_count, 10);
    assert!(config.requeue_on_nack);
    assert!(!config.enable_delayed_retry);
    assert!(config.retry_queue_name.is_none());
    assert!(config.retry_exchange_name.is_none());
    assert_eq!(config.max_retry_delay_ms, 3_600_000); // 1 hour
    assert_eq!(config.min_retry_delay_ms, 1_000); // 1 second
}

/// Test that delayed retry config can be enabled
#[test]
fn test_delayed_retry_config_enabled() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test-queue".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    assert!(config.enable_delayed_retry);
}

/// Test custom retry queue names
#[test]
fn test_custom_retry_names() {
    let config = RabbitMqConsumerConfig {
        queue_name: "main-queue".to_string(),
        enable_delayed_retry: true,
        retry_queue_name: Some("custom-retry".to_string()),
        retry_exchange_name: Some("custom-exchange".to_string()),
        ..Default::default()
    };

    assert_eq!(config.retry_queue_name, Some("custom-retry".to_string()));
    assert_eq!(
        config.retry_exchange_name,
        Some("custom-exchange".to_string())
    );
}

/// Mock AckHandle for testing retry_with_delay fallback
#[derive(Debug)]
struct MockAckHandle {
    nack_called: std::sync::Arc<std::sync::atomic::AtomicBool>,
    nack_requeue: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl MockAckHandle {
    fn new() -> (
        Self,
        std::sync::Arc<std::sync::atomic::AtomicBool>,
        std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) {
        let nack_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let nack_requeue = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        (
            Self {
                nack_called: nack_called.clone(),
                nack_requeue: nack_requeue.clone(),
            },
            nack_called,
            nack_requeue,
        )
    }
}

#[async_trait::async_trait]
impl AckHandle for MockAckHandle {
    async fn ack(&self) -> foxtive_worker::WorkerResult<()> {
        Ok(())
    }

    async fn nack(&self, requeue: bool) -> foxtive_worker::WorkerResult<()> {
        self.nack_called
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.nack_requeue
            .store(requeue, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

/// Test that default retry_with_delay falls back to nack(true)
#[tokio::test]
async fn test_default_retry_with_delay_fallback() {
    let (mock_handle, nack_called, nack_requeue) = MockAckHandle::new();

    let message = Message {
        id: "test-msg".to_string(),
        payload: serde_json::json!({"test": "data"}),
        metadata: MessageMetadata::new("test-queue"),
    };

    // Call retry_with_delay on the mock handle
    mock_handle.retry_with_delay(&message, 1000).await.unwrap();

    // Verify it fell back to nack with requeue=true
    assert!(nack_called.load(std::sync::atomic::Ordering::SeqCst));
    assert!(nack_requeue.load(std::sync::atomic::Ordering::SeqCst));
}

/// Test ReceivedMessage retry_with_delay delegation
#[tokio::test]
async fn test_received_message_retry_delegation() {
    use foxtive_worker::message::ReceivedMessage;

    let (mock_handle, nack_called, nack_requeue) = MockAckHandle::new();

    let message = Message {
        id: "test-msg".to_string(),
        payload: serde_json::json!({"test": "data"}),
        metadata: MessageMetadata::new("test-queue"),
    };

    let received = ReceivedMessage::new(message, Arc::new(mock_handle));

    // Call retry_with_delay through ReceivedMessage
    received.retry_with_delay(2000).await.unwrap();

    // Verify delegation worked
    assert!(nack_called.load(std::sync::atomic::Ordering::SeqCst));
    assert!(nack_requeue.load(std::sync::atomic::Ordering::SeqCst));
}

/// Test various delay values conversion
#[test]
fn test_delay_conversion() {
    use std::time::Duration;

    let delays = vec![
        Duration::from_millis(100),
        Duration::from_millis(1000),
        Duration::from_secs(5),
        Duration::from_secs(60),
    ];

    for delay in delays {
        let delay_ms = delay.as_millis() as u64;
        assert!(delay_ms > 0, "Delay should be positive");

        // Verify round-trip conversion
        let reconstructed = Duration::from_millis(delay_ms);
        assert_eq!(delay.as_millis(), reconstructed.as_millis());
    }
}

/// Test that delays below minimum are clamped up
#[test]
fn test_delay_clamping_min() {
    let config = RabbitMqConsumerConfig {
        min_retry_delay_ms: 1000,
        max_retry_delay_ms: 3_600_000,
        ..Default::default()
    };

    // Request 100ms delay (below minimum)
    let requested_delay = 100u64;
    let clamped = requested_delay
        .max(config.min_retry_delay_ms)
        .min(config.max_retry_delay_ms);

    assert_eq!(clamped, 1000, "Should clamp to minimum");
}

/// Test that delays above maximum are clamped down
#[test]
fn test_delay_clamping_max() {
    let config = RabbitMqConsumerConfig {
        min_retry_delay_ms: 1000,
        max_retry_delay_ms: 3_600_000,
        ..Default::default()
    };

    // Request 2 hour delay (above maximum)
    let requested_delay = 7_200_000u64;
    let clamped = requested_delay
        .max(config.min_retry_delay_ms)
        .min(config.max_retry_delay_ms);

    assert_eq!(clamped, 3_600_000, "Should clamp to maximum");
}

/// Test that delays within range are not modified
#[test]
fn test_delay_within_range_unchanged() {
    let config = RabbitMqConsumerConfig {
        min_retry_delay_ms: 1000,
        max_retry_delay_ms: 3_600_000,
        ..Default::default()
    };

    // Request 5 second delay (within range)
    let requested_delay = 5000u64;
    let clamped = requested_delay
        .max(config.min_retry_delay_ms)
        .min(config.max_retry_delay_ms);

    assert_eq!(clamped, 5000, "Should remain unchanged");
}

/// Test custom delay limits
#[test]
fn test_custom_delay_limits() {
    let config = RabbitMqConsumerConfig {
        min_retry_delay_ms: 500,   // 0.5 seconds
        max_retry_delay_ms: 30000, // 30 seconds
        ..Default::default()
    };

    assert_eq!(config.min_retry_delay_ms, 500);
    assert_eq!(config.max_retry_delay_ms, 30000);

    // Test clamping with custom limits
    let low_delay = 100u64;
    let clamped_low = low_delay
        .max(config.min_retry_delay_ms)
        .min(config.max_retry_delay_ms);
    assert_eq!(clamped_low, 500);

    let high_delay = 60000u64;
    let clamped_high = high_delay
        .max(config.min_retry_delay_ms)
        .min(config.max_retry_delay_ms);
    assert_eq!(clamped_high, 30000);

    let mid_delay = 10000u64;
    let clamped_mid = mid_delay
        .max(config.min_retry_delay_ms)
        .min(config.max_retry_delay_ms);
    assert_eq!(clamped_mid, 10000);
}
