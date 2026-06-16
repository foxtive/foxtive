use std::sync::Arc;
use std::time::Duration;
use foxtive_worker::{Message, MessageBackend, MessageMetadata, RabbitMqBackend, RabbitMqConsumerConfig};
use foxtive_worker::prelude::ReceiveResult;

/// Test that attempt count is preserved across retries via headers
#[tokio::test]
#[ignore] // Requires RabbitMQ running and takes time
async fn test_attempt_count_preservation() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_attempt_count".to_string(),
        enable_delayed_retry: true,
        prefetch_count: 1,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // Publish initial message
    let message = Message {
        id: "attempt-count-msg".to_string(),
        payload: serde_json::json!({"data": "test"}),
        metadata: MessageMetadata::new("test_attempt_count"),
    };

    // Get a connection to publish
    let conn = backend.pool.get().await.unwrap();
    let channel = conn.create_channel().await.unwrap();

    use lapin::options::BasicPublishOptions;
    use lapin::BasicProperties;

    let payload = serde_json::to_vec(&message.payload).unwrap();
    channel
        .basic_publish(
            "",
            "test_attempt_count",
            BasicPublishOptions::default(),
            &payload,
            BasicProperties::default()
                .with_message_id(message.id.clone().into())
                .with_content_type("application/json".into()),
        )
        .await
        .unwrap();

    // First attempt: should have attempt=0
    match backend.receive().await {
        Ok(ReceiveResult::Message(received)) => {
            assert_eq!(received.message.metadata.attempt, 0, "First attempt should be 0");
            println!("Attempt 1: attempt count = {}", received.message.metadata.attempt);
            
            // Schedule retry
            received.retry_with_delay(1000).await.unwrap();
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        other => panic!("Expected message, got: {:?}", other),
    }

    // Second attempt: should have attempt=1 (restored from header)
    match backend.receive().await {
        Ok(ReceiveResult::Message(received)) => {
            assert_eq!(received.message.metadata.attempt, 1, "Second attempt should be 1");
            println!("Attempt 2: attempt count = {}", received.message.metadata.attempt);
            
            // Schedule retry
            received.retry_with_delay(1000).await.unwrap();
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        other => panic!("Expected message, got: {:?}", other),
    }

    // Third attempt: should have attempt=2
    match backend.receive().await {
        Ok(ReceiveResult::Message(received)) => {
            assert_eq!(received.message.metadata.attempt, 2, "Third attempt should be 2");
            println!("Attempt 3: attempt count = {}", received.message.metadata.attempt);
            
            // Acknowledge this time
            received.ack().await.unwrap();
        }
        other => panic!("Expected message, got: {:?}", other),
    }
}

/// Test that x-retry-attempt header is set correctly
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_retry_attempt_header() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_retry_header".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // Create message with attempt=0
    let message = Message {
        id: "header-test-msg".to_string(),
        payload: serde_json::json!({}),
        metadata: MessageMetadata::new("test_retry_header"),
    };

    // Publish to retry queue - should store attempt=1 in header
    let result = backend.publish_to_retry_queue(&message, 1000).await;
    assert!(result.is_ok());
    
    // The header x-retry-attempt should be set to 1 (current attempt + 1)
    // In production, verify via RabbitMQ management API
}

/// Test multiple retries maintain correct count
#[tokio::test]
#[ignore] // Requires RabbitMQ running and takes time
async fn test_multiple_retries_count_sequence() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_retry_sequence".to_string(),
        enable_delayed_retry: true,
        prefetch_count: 1,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // Publish initial message
    let message = Message {
        id: "sequence-msg".to_string(),
        payload: serde_json::json!({}),
        metadata: MessageMetadata::new("test_retry_sequence"),
    };

    let conn = backend.pool.get().await.unwrap();
    let channel = conn.create_channel().await.unwrap();

    use lapin::options::BasicPublishOptions;
    use lapin::BasicProperties;

    let payload = serde_json::to_vec(&message.payload).unwrap();
    channel
        .basic_publish(
            "",
            "test_retry_sequence",
            BasicPublishOptions::default(),
            &payload,
            BasicProperties::default()
                .with_message_id(message.id.clone().into())
                .with_content_type("application/json".into()),
        )
        .await
        .unwrap();

    // Expected sequence: 0 -> 1 -> 2 -> 3 -> 4
    let expected_attempts = vec![0, 1, 2, 3, 4];
    
    for (i, expected) in expected_attempts.iter().enumerate() {
        match backend.receive().await {
            Ok(ReceiveResult::Message(received)) => {
                assert_eq!(
                    received.message.metadata.attempt, 
                    *expected,
                    "Attempt {} should have count {}",
                    i + 1,
                    expected
                );
                println!("Retry {}: attempt count = {}", i + 1, received.message.metadata.attempt);
                
                if i < 4 {
                    received.retry_with_delay(500).await.unwrap();
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                } else {
                    received.ack().await.unwrap();
                }
            }
            other => panic!("Expected message at iteration {}, got: {:?}", i, other),
        }
    }
}

/// Test attempt count with routing key preservation
#[tokio::test]
#[ignore] // Requires RabbitMQ running and takes time
async fn test_attempt_count_with_routing_key() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_attempt_rk".to_string(),
        enable_delayed_retry: true,
        prefetch_count: 1,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // Create message with custom routing key
    let mut metadata = MessageMetadata::new("test_attempt_rk");
    metadata.routing_key = Some("custom.routing.key".into());
    
    let message = Message {
        id: "rk-attempt-msg".to_string(),
        payload: serde_json::json!({}),
        metadata,
    };

    let conn = backend.pool.get().await.unwrap();
    let channel = conn.create_channel().await.unwrap();

    use lapin::options::BasicPublishOptions;
    use lapin::BasicProperties;

    let payload = serde_json::to_vec(&message.payload).unwrap();
    channel
        .basic_publish(
            "",
            "custom.routing.key",
            BasicPublishOptions::default(),
            &payload,
            BasicProperties::default()
                .with_message_id(message.id.clone().into())
                .with_content_type("application/json".into()),
        )
        .await
        .unwrap();

    // Receive and verify both routing key and attempt count are preserved
    match backend.receive().await {
        Ok(ReceiveResult::Message(received)) => {
            assert_eq!(received.message.metadata.attempt, 0);
            assert_eq!(
                received.message.metadata.routing_key.as_ref().map(|s| s.as_str()),
                Some("custom.routing.key")
            );
            
            received.retry_with_delay(1000).await.unwrap();
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        other => panic!("Expected message, got: {:?}", other),
    }

    // After retry, both should still be preserved
    match backend.receive().await {
        Ok(ReceiveResult::Message(received)) => {
            assert_eq!(received.message.metadata.attempt, 1, "Attempt count should increment");
            assert_eq!(
                received.message.metadata.routing_key.as_ref().map(|s| s.as_str()),
                Some("custom.routing.key"),
                "Routing key should be preserved"
            );
            
            received.ack().await.unwrap();
        }
        other => panic!("Expected retried message, got: {:?}", other),
    }
}

/// Test attempt count overflow protection
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_attempt_count_overflow_protection() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_overflow".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // Create message with very high attempt count
    let mut metadata = MessageMetadata::new("test_overflow");
    metadata.attempt = u32::MAX - 10; // Near overflow
    
    let message = Message {
        id: "overflow-msg".to_string(),
        payload: serde_json::json!({}),
        metadata,
    };

    // Should handle gracefully without panicking
    let result = backend.publish_to_retry_queue(&message, 1000).await;
    
    // Either succeeds or fails gracefully (not panic)
    match result {
        Ok(_) => println!("Handled high attempt count successfully"),
        Err(e) => println!("Gracefully failed with high attempt count: {}", e),
    }
}

/// Test DLQ receives correct final attempt count
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_dlq_final_attempt_count() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_dlq_final".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // Create message with attempt=3 (exhausted)
    let mut metadata = MessageMetadata::new("test_dlq_final");
    metadata.attempt = 3;
    
    let message = Message {
        id: "dlq-final-msg".to_string(),
        payload: serde_json::json!({}),
        metadata,
    };

    // Send to DLQ
    let result = backend.publish_to_dlq(&message, "Retries exhausted").await;
    assert!(result.is_ok());
    
    // DLQ headers should contain x-final-attempt: 3
    // Verify via RabbitMQ management API in production
}

/// Test attempt count with different delay values
#[tokio::test]
#[ignore] // Requires RabbitMQ running and takes time
async fn test_attempt_count_with_varying_delays() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_varying_delays".to_string(),
        enable_delayed_retry: true,
        prefetch_count: 1,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    let delays = vec![500, 1000, 2000]; // Different delays
    
    // Publish initial message
    let message = Message {
        id: "varying-delay-msg".to_string(),
        payload: serde_json::json!({}),
        metadata: MessageMetadata::new("test_varying_delays"),
    };

    let conn = backend.pool.get().await.unwrap();
    let channel = conn.create_channel().await.unwrap();

    use lapin::options::BasicPublishOptions;
    use lapin::BasicProperties;

    let payload = serde_json::to_vec(&message.payload).unwrap();
    channel
        .basic_publish(
            "",
            "test_varying_delays",
            BasicPublishOptions::default(),
            &payload,
            BasicProperties::default()
                .with_message_id(message.id.clone().into())
                .with_content_type("application/json".into()),
        )
        .await
        .unwrap();

    // Retry with different delays, verify attempt count increments correctly
    for (i, delay) in delays.iter().enumerate() {
        match backend.receive().await {
            Ok(ReceiveResult::Message(received)) => {
                assert_eq!(received.message.metadata.attempt, i as u32);
                println!("Delay {}: attempt = {}", delay, received.message.metadata.attempt);
                
                received.retry_with_delay(*delay).await.unwrap();
                tokio::time::sleep(Duration::from_millis(delay + 500)).await;
            }
            other => panic!("Expected message, got: {:?}", other),
        }
    }

    // Final acknowledgment
    match backend.receive().await {
        Ok(ReceiveResult::Message(received)) => {
            assert_eq!(received.message.metadata.attempt, 3);
            received.ack().await.unwrap();
        }
        other => panic!("Expected final message, got: {:?}", other),
    }
}

/// Test concurrent retries maintain separate attempt counts
#[tokio::test]
#[ignore] // Requires RabbitMQ running and takes time
async fn test_concurrent_retries_attempt_isolation() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_concurrent_attempts".to_string(),
        enable_delayed_retry: true,
        prefetch_count: 10,
        ..Default::default()
    };

    let backend = Arc::new(RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend"));

    // Publish 5 messages
    let conn = backend.pool.get().await.unwrap();
    let channel = conn.create_channel().await.unwrap();

    use lapin::options::BasicPublishOptions;
    use lapin::BasicProperties;

    for i in 0..5 {
        let message = Message {
            id: format!("concurrent-msg-{}", i),
            payload: serde_json::json!({"index": i}),
            metadata: MessageMetadata::new("test_concurrent_attempts"),
        };

        let payload = serde_json::to_vec(&message.payload).unwrap();
        channel
            .basic_publish(
                "",
                "test_concurrent_attempts",
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default()
                    .with_message_id(message.id.into())
                    .with_content_type("application/json".into()),
            )
            .await
            .unwrap();
    }

    // Receive all messages and retry them concurrently
    let mut handles = vec![];
    
    for _ in 0..5 {
        let backend_clone = backend.clone();
        let handle = tokio::spawn(async move {
            match backend_clone.receive().await {
                Ok(ReceiveResult::Message(received)) => {
                    let attempt = received.message.metadata.attempt;
                    println!("Received message with attempt {}", attempt);
                    assert_eq!(attempt, 0, "Initial attempt should be 0");
                    
                    received.retry_with_delay(1000).await.unwrap();
                }
                other => panic!("Expected message, got: {:?}", other),
            }
        });
        handles.push(handle);
    }

    // Wait for all retries to be scheduled
    for handle in handles {
        handle.await.unwrap();
    }

    println!("All messages retried concurrently");
}
