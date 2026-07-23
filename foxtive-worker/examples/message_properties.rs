//! Example demonstrating message properties usage in foxtive-worker
//!
//! This example shows how to work with message properties for microservices architectures,
//! including setting custom metadata, extracting backend-specific properties, and using
//! them for distributed tracing.

use foxtive_worker::error::WorkerResult;
use foxtive_worker::prelude::*;
use serde_json::json;

#[tokio::main]
async fn main() -> WorkerResult<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("=== Message Properties Examples ===\n");

    // Example 1: Creating messages with custom properties (Memory backend)
    example_custom_properties().await?;

    // Example 2: Microservice identification
    example_microservice_tracking().await?;

    // Example 3: Distributed tracing with correlation IDs
    example_distributed_tracing().await?;

    // Example 4: Priority-based processing
    example_priority_processing().await?;

    // Example 5: TTL/Expiration handling
    example_expiration().await?;

    Ok(())
}

/// Example 1: Creating messages with custom properties
async fn example_custom_properties() -> WorkerResult<()> {
    println!("1. Custom Properties with Memory Backend");
    println!("-----------------------------------------");

    let backend = MemoryBackend::with_source("example-queue");

    // Create a message with custom properties
    let properties = MessageProperties::new()
        .with_content_type("application/json")
        .with_app_id("user-service")
        .with_message_type("user.created")
        .with_header("correlation_id", "trace-abc-123")
        .with_header("environment", "production");

    let message_id = backend.enqueue_with_properties(
        json!({
            "user_id": 12345,
            "email": "user@example.com"
        }),
        Some(properties),
    );

    println!("✓ Enqueued message with ID: {}", message_id);
    println!("  - Content Type: application/json");
    println!("  - App ID: user-service");
    println!("  - Message Type: user.created");
    println!("  - Correlation ID: trace-abc-123\n");

    Ok(())
}

/// Example 2: Microservice identification and tracking
async fn example_microservice_tracking() -> WorkerResult<()> {
    println!("2. Microservice Identification");
    println!("-------------------------------");

    let backend = MemoryBackend::with_source("events");

    // Track which service sent the message
    let properties = MessageProperties::new()
        .with_app_id("payment-service")
        .with_user_id("user-789")
        .with_cluster_id("us-east-1")
        .with_header("service_version", "v2.1.0")
        .with_header("deployment_id", "deploy-456");

    backend.enqueue_with_properties(
        json!({
            "event": "payment.completed",
            "amount": 99.99,
            "currency": "USD"
        }),
        Some(properties),
    );

    println!("✓ Payment event tracked:");
    println!("  - Source Service: payment-service");
    println!("  - User: user-789");
    println!("  - Cluster: us-east-1");
    println!("  - Version: v2.1.0\n");

    Ok(())
}

/// Example 3: Distributed tracing with correlation IDs
async fn example_distributed_tracing() -> WorkerResult<()> {
    println!("3. Distributed Tracing");
    println!("-----------------------");

    let correlation_id = "trace-xyz-789";

    // Simulate a request flowing through multiple services
    let services = vec![
        ("api-gateway", "request.received"),
        ("auth-service", "auth.validated"),
        ("order-service", "order.created"),
        ("notification-service", "notification.sent"),
    ];

    for (service, event_type) in services {
        let backend = MemoryBackend::with_source(format!("{}-queue", service));

        let properties = MessageProperties::new()
            .with_app_id(service)
            .with_message_type(event_type)
            .with_header("correlation_id", correlation_id)
            .with_header("span_id", format!("span-{}", service));

        backend.enqueue_with_properties(
            json!({
                "event": event_type,
                "timestamp": chrono::Utc::now().to_rfc3339()
            }),
            Some(properties),
        );

        println!("✓ {} → {}", service, event_type);
    }

    println!("\nAll events share correlation_id: {}\n", correlation_id);

    Ok(())
}

/// Example 4: Priority-based message processing
async fn example_priority_processing() -> WorkerResult<()> {
    println!("4. Priority-Based Processing");
    println!("-----------------------------");

    let backend = MemoryBackend::with_source("priority-queue");

    // High priority message (e.g., fraud detection)
    let high_priority = MessageProperties::new()
        .with_priority(10)
        .with_message_type("fraud.alert")
        .with_app_id("security-service");

    backend.enqueue_with_properties(
        json!({"alert": "suspicious_activity", "severity": "high"}),
        Some(high_priority),
    );
    println!("✓ High priority message (priority=10): Fraud alert");

    // Medium priority message (e.g., order processing)
    let medium_priority = MessageProperties::new()
        .with_priority(5)
        .with_message_type("order.processed")
        .with_app_id("order-service");

    backend.enqueue_with_properties(
        json!({"order_id": "ORD-123", "status": "shipped"}),
        Some(medium_priority),
    );
    println!("✓ Medium priority message (priority=5): Order shipped");

    // Low priority message (e.g., analytics)
    let low_priority = MessageProperties::new()
        .with_priority(1)
        .with_message_type("analytics.event")
        .with_app_id("analytics-service");

    backend.enqueue_with_properties(
        json!({"event": "page_view", "page": "/home"}),
        Some(low_priority),
    );
    println!("✓ Low priority message (priority=1): Analytics event\n");

    Ok(())
}

/// Example 5: Message expiration/TTL
async fn example_expiration() -> WorkerResult<()> {
    println!("5. Message Expiration (TTL)");
    println!("----------------------------");

    let backend = MemoryBackend::with_source("ttl-queue");

    // Short-lived message (e.g., OTP code - expires in 5 minutes)
    let otp_properties = MessageProperties::new()
        .with_message_type("otp.code")
        .with_app_id("auth-service")
        .with_expiration(300000); // 5 minutes in milliseconds

    backend.enqueue_with_properties(
        json!({"otp": "123456", "phone": "+1234567890"}),
        Some(otp_properties),
    );
    println!("✓ OTP message: Expires in 5 minutes (300,000ms)");

    // Long-lived message (e.g., audit log - expires in 24 hours)
    let audit_properties = MessageProperties::new()
        .with_message_type("audit.log")
        .with_app_id("compliance-service")
        .with_expiration(86400000); // 24 hours in milliseconds

    backend.enqueue_with_properties(
        json!({"action": "login", "user": "admin", "ip": "192.168.1.1"}),
        Some(audit_properties),
    );
    println!("✓ Audit log: Expires in 24 hours (86,400,000ms)\n");

    Ok(())
}

/// Example 6: Accessing properties in a worker (demonstration)
#[allow(dead_code)]
async fn example_worker_usage(received: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
    // Access message properties in your worker
    if let Some(props) = &received.message.metadata.properties {
        // Get standard fields
        if let Some(app_id) = &props.app_id {
            tracing::info!("Message from service: {}", app_id);
        }

        if let Some(message_type) = &props.message_type {
            tracing::info!("Message type: {}", message_type);
        }

        // Get custom headers for distributed tracing
        if let Some(headers) = &props.headers {
            if let Some(correlation_id) = headers.get("correlation_id") {
                tracing::info!("Correlation ID: {}", correlation_id);
            }

            if let Some(environment) = headers.get("environment") {
                tracing::info!("Environment: {}", environment);
            }
        }

        // Priority-based processing logic
        if let Some(priority) = props.priority
            && priority >= 8
        {
            tracing::warn!("High priority message detected!");
            // Process immediately
        }
    }

    received.ack().await?;
    Ok(())
}
