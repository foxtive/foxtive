use std::time::Duration;
use foxtive_worker::{Message, MessageBackend, MessageMetadata, RabbitMqBackend, RabbitMqConsumerConfig};
use foxtive_worker::prelude::ReceiveResult;

/// Test that retry infrastructure is set up correctly with DLX and TTL
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_retry_infrastructure_setup() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_retry_queue".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // Verify retry queue and exchange names are set
    assert!(backend.retry_queue_name.is_some());
    assert!(backend.retry_exchange_name.is_some());

    let retry_queue = backend.retry_queue_name.as_ref().unwrap();
    let retry_exchange = backend.retry_exchange_name.as_ref().unwrap();

    assert_eq!(retry_queue, "test_retry_queue_retry");
    assert_eq!(retry_exchange, "test_retry_queue_retry_exchange");

    // Health check should pass
    assert!(backend.health_check().await.is_ok());
}

/// Test publishing to retry queue with delay
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_publish_to_retry_queue() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_retry_pub_queue".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // Create a test message
    let message = Message {
        id: "test-msg-1".to_string(),
        payload: serde_json::json!({"test": "data"}),
        metadata: MessageMetadata::new("test_retry_pub_queue"),
    };

    // Publish to retry queue with 1 second delay
    let result = backend.publish_to_retry_queue(&message, 1000).await;
    assert!(result.is_ok(), "Failed to publish to retry queue: {:?}", result);

    // Wait for TTL to expire and message to be dead-lettered back
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Try to receive the message from the main queue
    // Note: This would require a consumer setup, which is complex for this test
    // In production, you'd verify via RabbitMQ management API or by consuming
}

/// Test that messages without delayed retry enabled fall back to nack
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_retry_without_delayed_retry_enabled() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_no_delay_queue".to_string(),
        enable_delayed_retry: false, // Disabled
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // Retry queue should not be configured
    assert!(backend.retry_queue_name.is_none());
    assert!(backend.retry_exchange_name.is_none());

    // Attempting to publish to retry queue should fail
    let message = Message {
        id: "test-msg-2".to_string(),
        payload: serde_json::json!({"test": "data"}),
        metadata: MessageMetadata::new("test_no_delay_queue"),
    };

    let result = backend.publish_to_retry_queue(&message, 1000).await;
    assert!(result.is_err(), "Should fail when delayed retry is disabled");
}

/// Test custom retry queue and exchange names
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_custom_retry_names() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_custom_queue".to_string(),
        enable_delayed_retry: true,
        retry_queue_name: Some("my_custom_retry_q".to_string()),
        retry_exchange_name: Some("my_custom_retry_x".to_string()),
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    assert_eq!(
        backend.retry_queue_name.as_ref().unwrap(),
        "my_custom_retry_q"
    );
    assert_eq!(
        backend.retry_exchange_name.as_ref().unwrap(),
        "my_custom_retry_x"
    );
}

/// Test different delay values
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_various_delay_values() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_delays_queue".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    let delays = vec![100, 1000, 5000, 60000]; // 100ms, 1s, 5s, 60s

    for (i, delay) in delays.iter().enumerate() {
        let message = Message {
            id: format!("test-delay-{}", i),
            payload: serde_json::json!({"delay": delay}),
            metadata: MessageMetadata::new("test_delays_queue"),
        };

        let result = backend.publish_to_retry_queue(&message, *delay).await;
        assert!(
            result.is_ok(),
            "Failed to publish with delay {}: {:?}",
            delay,
            result
        );
    }
}

/// Integration test: Verify end-to-end delayed retry flow
#[tokio::test]
#[ignore] // Requires RabbitMQ running and takes time
async fn test_end_to_end_delayed_retry() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_e2e_retry".to_string(),
        enable_delayed_retry: true,
        prefetch_count: 1,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // Publish initial message
    let message = Message {
        id: "e2e-test-msg".to_string(),
        payload: serde_json::json!({"attempt": 1}),
        metadata: MessageMetadata::new("test_e2e_retry"),
    };

    // Get a connection to publish the initial message
    let conn = backend.pool.get().await.unwrap();
    let channel = conn.create_channel().await.unwrap();

    use lapin::options::BasicPublishOptions;
    use lapin::BasicProperties;

    let payload = serde_json::to_vec(&message.payload).unwrap();
    channel
        .basic_publish(
            "",
            "test_e2e_retry",
            BasicPublishOptions::default(),
            &payload,
            BasicProperties::default()
                .with_message_id(message.id.clone().into())
                .with_content_type("application/json".into()),
        )
        .await
        .unwrap();

    // Receive the message
    match backend.receive().await {
        Ok(ReceiveResult::Message(received)) => {
            println!("Received message: {}", received.message.id);

            // Simulate failure and schedule retry with 2 second delay
            let delay_ms = 2000u64;
            received
                .retry_with_delay(delay_ms)
                .await
                .expect("Failed to schedule retry");

            println!("Scheduled retry with {}ms delay", delay_ms);

            // Wait for TTL + some buffer
            tokio::time::sleep(Duration::from_millis(delay_ms + 500)).await;

            // Receive the retried message
            match backend.receive().await {
                Ok(ReceiveResult::Message(retried)) => {
                    println!("Retried message received: {}", retried.message.id);
                    assert_eq!(retried.message.id, message.id);
                    
                    // Acknowledge the retried message
                    retried.ack().await.unwrap();
                }
                other => panic!("Expected message after retry, got: {:?}", other),
            }
        }
        other => panic!("Expected initial message, got: {:?}", other),
    }
}
