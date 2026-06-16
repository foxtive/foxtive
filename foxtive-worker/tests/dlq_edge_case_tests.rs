use std::sync::Arc;
use std::time::Duration;
use foxtive_worker::{Message, MessageBackend, MessageMetadata, RabbitMqBackend, RabbitMqConsumerConfig};
use foxtive_worker::prelude::ReceiveResult;

/// Test DLQ is created when delayed retry is enabled
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_dlq_creation_with_delayed_retry() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_dlq_creation".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // Verify DLQ name is set
    assert!(backend.dlq_name.is_some());
    
    let dlq_name = backend.dlq_name.as_ref().unwrap();
    assert_eq!(dlq_name, "test_dlq_creation-dlq");
    
    // Health check should pass
    assert!(backend.health_check().await.is_ok());
}

/// Test publishing to DLQ with failure metadata
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_publish_to_dlq() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_dlq_publish".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // Create a test message with attempt count
    let mut metadata = MessageMetadata::new("test_dlq_publish");
    metadata.attempt = 3; // Exhausted retries
    
    let message = Message {
        id: "test-dlq-msg-1".to_string(),
        payload: serde_json::json!({"test": "failed_data"}),
        metadata,
    };

    // Publish to DLQ with error message
    let error_msg = "Connection timeout after 3 retries";
    let result = backend.publish_to_dlq(&message, error_msg).await;
    assert!(result.is_ok(), "Failed to publish to DLQ: {:?}", result);
}

/// Test DLQ headers contain failure metadata
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_dlq_headers_metadata() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_dlq_headers".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    let mut metadata = MessageMetadata::new("test_dlq_headers");
    metadata.attempt = 2;
    metadata.routing_key = Some("test.routing.key".into());
    
    let message = Message {
        id: "test-dlq-headers".to_string(),
        payload: serde_json::json!({"data": "value"}),
        metadata,
    };

    // Publish to DLQ
    let result = backend.publish_to_dlq(&message, "Test failure").await;
    assert!(result.is_ok());
    
    // In production, you'd verify headers via RabbitMQ management API
    // Headers should include:
    // - x-original-routing-key: "test.routing.key"
    // - x-failure-reason: "Test failure"
    // - x-final-attempt: 2
    // - x-failed-at: ISO 8601 timestamp
}

/// Test DLQ fails gracefully when not configured
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_dlq_not_configured_error() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_no_dlq".to_string(),
        enable_delayed_retry: false, // DLQ not enabled
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // DLQ should not be configured
    assert!(backend.dlq_name.is_none());

    let message = Message {
        id: "test-no-dlq".to_string(),
        payload: serde_json::json!({}),
        metadata: MessageMetadata::new("test_no_dlq"),
    };

    // Should fail when trying to publish to non-existent DLQ
    let result = backend.publish_to_dlq(&message, "Error").await;
    assert!(result.is_err(), "Should fail when DLQ is not configured");
}

/// Test retry infrastructure includes DLQ
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_retry_infrastructure_includes_dlq() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_full_infra".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // All three components should be present
    assert!(backend.retry_queue_name.is_some());
    assert!(backend.retry_exchange_name.is_some());
    assert!(backend.dlq_name.is_some());
    
    println!("Retry queue: {:?}", backend.retry_queue_name);
    println!("Retry exchange: {:?}", backend.retry_exchange_name);
    println!("DLQ: {:?}", backend.dlq_name);
}

/// Test DLQ naming convention matches pattern
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_dlq_naming_convention() {
    let queue_names = vec![
        "pigeon-queue-auth",
        "email-notifications",
        "payment-processing",
        "user-events",
    ];

    for queue_name in queue_names {
        let config = RabbitMqConsumerConfig {
            queue_name: queue_name.to_string(),
            enable_delayed_retry: true,
            ..Default::default()
        };

        let backend = RabbitMqBackend::new("amqp://localhost", config)
            .await
            .expect("Failed to create backend");

        let expected_dlq = format!("{}-dlq", queue_name);
        assert_eq!(backend.dlq_name.as_ref().unwrap(), &expected_dlq);
    }
}

/// Test multiple messages to DLQ
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_multiple_messages_to_dlq() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_multi_dlq".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // Publish multiple failed messages to DLQ
    for i in 0..5 {
        let mut metadata = MessageMetadata::new("test_multi_dlq");
        metadata.attempt = 3;
        
        let message = Message {
            id: format!("failed-msg-{}", i),
            payload: serde_json::json!({"index": i}),
            metadata,
        };

        let result = backend.publish_to_dlq(&message, &format!("Failure {}", i)).await;
        assert!(result.is_ok(), "Failed to publish message {} to DLQ", i);
    }
}

/// Test DLQ with different error types
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_dlq_various_error_messages() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_dlq_errors".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    let errors = vec![
        "Connection timeout",
        "Database connection refused",
        "HTTP 500 Internal Server Error",
        "Serialization error: invalid JSON",
        "Authentication failed: invalid token",
        "Rate limit exceeded",
        "Out of memory",
    ];

    for (i, error) in errors.iter().enumerate() {
        let message = Message {
            id: format!("error-msg-{}", i),
            payload: serde_json::json!({"error_type": error}),
            metadata: MessageMetadata::new("test_dlq_errors"),
        };

        let result = backend.publish_to_dlq(&message, error).await;
        assert!(result.is_ok(), "Failed with error type: {}", error);
    }
}

/// Test DLQ with large payloads
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_dlq_large_payloads() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_dlq_large".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // Create a large payload (1MB)
    let large_data = "x".repeat(1_048_576);
    let message = Message {
        id: "large-payload-msg".to_string(),
        payload: serde_json::json!({"data": large_data}),
        metadata: MessageMetadata::new("test_dlq_large"),
    };

    let result = backend.publish_to_dlq(&message, "Large payload failure").await;
    assert!(result.is_ok(), "Failed to publish large payload to DLQ");
}

/// Test DLQ preserves message properties
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_dlq_preserves_properties() {
    use foxtive_worker::MessageProperties;

    let config = RabbitMqConsumerConfig {
        queue_name: "test_dlq_props".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    let mut metadata = MessageMetadata::new("test_dlq_props");
    metadata.properties = Some(MessageProperties::new()
        .with_app_id("test-service")
        .with_message_type("test.event")
        .with_priority(5)
        .with_header("correlation_id", "trace-123"));
    
    let message = Message {
        id: "props-msg".to_string(),
        payload: serde_json::json!({"test": "data"}),
        metadata,
    };

    let result = backend.publish_to_dlq(&message, "Property preservation test").await;
    assert!(result.is_ok());
    
    // Note: Properties are in message.metadata but DLQ headers focus on failure info
    // The full message payload (including metadata) is preserved in the DLQ message body
}

/// Integration test: Full retry flow ending in DLQ
#[tokio::test]
#[ignore] // Requires RabbitMQ running and takes time
async fn test_full_retry_flow_to_dlq() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_full_flow".to_string(),
        enable_delayed_retry: true,
        prefetch_count: 1,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    // Publish initial message
    let message = Message {
        id: "full-flow-msg".to_string(),
        payload: serde_json::json!({"attempt": 0}),
        metadata: MessageMetadata::new("test_full_flow"),
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
            "test_full_flow",
            BasicPublishOptions::default(),
            &payload,
            BasicProperties::default()
                .with_message_id(message.id.clone().into())
                .with_content_type("application/json".into()),
        )
        .await
        .unwrap();

    // Receive and simulate 3 failures
    for attempt in 0..3 {
        match backend.receive().await {
            Ok(ReceiveResult::Message(received)) => {
                println!("Attempt {}: Received message {}", attempt + 1, received.message.id);

                if attempt < 2 {
                    // First 2 attempts: retry with delay
                    received
                        .retry_with_delay(1000) // 1 second delay
                        .await
                        .expect("Failed to schedule retry");
                    
                    // Wait for TTL
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                } else {
                    // 3rd attempt: exhausted, send to DLQ
                    received
                        .send_to_dlq("All retries exhausted")
                        .await
                        .expect("Failed to send to DLQ");
                    
                    println!("Message sent to DLQ after 3 attempts");
                }
            }
            other => panic!("Expected message, got: {:?}", other),
        }
    }

    // Verify DLQ has the message (would need RabbitMQ management API or consumer)
    println!("Test complete - check DLQ 'test_full_flow-dlq' for the failed message");
}

/// Test DLQ with special characters in error messages
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_dlq_special_characters() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_dlq_special".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    let backend = RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend");

    let special_errors = vec![
        "Error with \"quotes\"",
        "Error with 'apostrophes'",
        "Error with \\ backslashes",
        "Error with\nnewlines",
        "Error with\ttabs",
        "Error with unicode: 你好世界 🦊",
        "Error with emojis: ❌💥🔥",
    ];

    for (i, error) in special_errors.iter().enumerate() {
        let message = Message {
            id: format!("special-msg-{}", i),
            payload: serde_json::json!({}),
            metadata: MessageMetadata::new("test_dlq_special"),
        };

        let result = backend.publish_to_dlq(&message, error).await;
        assert!(result.is_ok(), "Failed with special chars: {}", error);
    }
}

/// Test concurrent DLQ publishing
#[tokio::test]
#[ignore] // Requires RabbitMQ running
async fn test_concurrent_dlq_publishing() {
    let config = RabbitMqConsumerConfig {
        queue_name: "test_dlq_concurrent".to_string(),
        enable_delayed_retry: true,
        ..Default::default()
    };

    let backend = Arc::new(RabbitMqBackend::new("amqp://localhost", config)
        .await
        .expect("Failed to create backend"));

    // Spawn multiple tasks publishing to DLQ concurrently
    let mut handles = vec![];
    
    for i in 0..10 {
        let backend_clone = backend.clone();
        let handle = tokio::spawn(async move {
            let message = Message {
                id: format!("concurrent-msg-{}", i),
                payload: serde_json::json!({"task": i}),
                metadata: MessageMetadata::new("test_dlq_concurrent"),
            };

            backend_clone
                .publish_to_dlq(&message, &format!("Concurrent failure {}", i))
                .await
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for (i, handle) in handles.into_iter().enumerate() {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "Task {} failed to publish to DLQ", i);
    }
}
