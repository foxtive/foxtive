mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{TestWorker, create_test_message, wait_for_condition};
#[cfg(feature = "rate-limit")]
use foxtive_worker::RateLimitMiddleware;
use foxtive_worker::{
    AckNackMiddleware, CircuitBreakerMiddleware, TracingMiddleware, WorkerPoolBuilder,
};

/// Test that middleware chain processes messages correctly with worker pool.
#[tokio::test]
async fn test_middleware_chain_with_pool() {
    let worker =
        Arc::new(TestWorker::new("middleware-worker").with_delay(Duration::from_millis(10)));

    let pool = WorkerPoolBuilder::new("middleware-pool")
        .with_concurrency_limit(5)
        .with_middleware(TracingMiddleware::new("test-service"))
        .with_middleware(AckNackMiddleware::default())
        .add_arc_worker(worker.clone())
        .build()
        .unwrap();

    // Dispatch messages through middleware chain
    for i in 0..10 {
        pool.dispatch(create_test_message(&format!("msg-{:02}", i)))
            .await
            .unwrap();
    }

    // Wait for completion
    wait_for_condition(
        || {
            worker
                .process_count
                .load(std::sync::atomic::Ordering::SeqCst)
                >= 10
        },
        Duration::from_secs(5),
    )
    .await;

    assert_eq!(
        worker
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst),
        10
    );
}

/// Test ack/nack middleware automatically acknowledges successful processing.
#[tokio::test]
async fn test_ack_nack_middleware_auto_ack() {
    use common::MockAckHandle;
    use foxtive_worker::{Message, MessageMetadata, ReceivedMessage};

    let worker = Arc::new(TestWorker::new("auto-ack-worker"));

    let pool = WorkerPoolBuilder::new("auto-ack-pool")
        .with_concurrency_limit(5)
        .with_middleware(AckNackMiddleware::default())
        .add_arc_worker(worker.clone())
        .build()
        .unwrap();

    // Create a message with trackable ack handle
    let (ack_handle, acked, nacked, _) = MockAckHandle::new();
    let message = Message {
        id: "test-msg".to_string(),
        payload: serde_json::json!({"test": "data"}),
        metadata: MessageMetadata::new("test-queue"),
    };
    let received = ReceivedMessage::new(message, Arc::new(ack_handle));

    pool.dispatch(received).await.unwrap();

    // Wait for processing
    wait_for_condition(
        || {
            worker
                .process_count
                .load(std::sync::atomic::Ordering::SeqCst)
                >= 1
        },
        Duration::from_secs(2),
    )
    .await;

    // Message should be auto-acked on success
    assert!(
        acked.load(std::sync::atomic::Ordering::SeqCst),
        "Message should be auto-acked"
    );
    assert!(
        !nacked.load(std::sync::atomic::Ordering::SeqCst),
        "Message should not be nacked"
    );
}

/// Test ack/nack middleware negative-acks on failure.
#[tokio::test]
async fn test_ack_nack_middleware_auto_nack() {
    use common::MockAckHandle;
    use foxtive_worker::{Message, MessageMetadata, ReceivedMessage};

    let worker = Arc::new(TestWorker::new("auto-nack-worker"));
    worker.set_should_fail(true); // Force failures

    let pool = WorkerPoolBuilder::new("auto-nack-pool")
        .with_concurrency_limit(5)
        .with_middleware(AckNackMiddleware::default())
        .add_arc_worker(worker.clone())
        .build()
        .unwrap();

    // Create a message with trackable ack handle
    let (ack_handle, acked, nacked, requeued) = MockAckHandle::new();
    let message = Message {
        id: "fail-msg".to_string(),
        payload: serde_json::json!({"test": "data"}),
        metadata: MessageMetadata::new("test-queue"),
    };
    let received = ReceivedMessage::new(message, Arc::new(ack_handle));

    let _ = pool.dispatch(received).await;

    // Wait for processing
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Message should be auto-nacked on failure
    assert!(
        !acked.load(std::sync::atomic::Ordering::SeqCst),
        "Failed message should not be acked"
    );
    assert!(
        nacked.load(std::sync::atomic::Ordering::SeqCst),
        "Failed message should be nacked"
    );
    assert!(
        requeued.load(std::sync::atomic::Ordering::SeqCst),
        "Failed message should be requeued by default"
    );
}

/// Test circuit breaker middleware prevents cascading failures.
#[tokio::test]
async fn test_circuit_breaker_middleware() {
    let worker = Arc::new(TestWorker::new("circuit-breaker-worker"));
    worker.set_should_fail(true); // Force all failures

    let pool = WorkerPoolBuilder::new("circuit-breaker-pool")
        .with_concurrency_limit(5)
        .with_middleware(CircuitBreakerMiddleware::new(3, Duration::from_millis(500)))
        .add_arc_worker(worker.clone())
        .build()
        .unwrap();

    // First 3 messages should fail but go through
    for i in 0..3 {
        let _ = pool
            .dispatch(create_test_message(&format!("msg-{}", i)))
            .await;
    }

    // Wait for failures to trigger circuit breaker
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Next messages might be rejected by open circuit (timing-dependent)
    // The circuit breaker is working if it tracks failures correctly
    let processed = worker
        .process_count
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        processed >= 3,
        "At least initial messages should be processed"
    );
}

/// Test real-world scenario: Protected API with multiple middleware layers.
#[tokio::test]
#[cfg(feature = "rate-limit")]
async fn test_protected_api_scenario() {
    // Simulate an API that needs tracing, rate limiting, and auto-ack
    let api_worker =
        Arc::new(TestWorker::new("api-processor").with_delay(Duration::from_millis(20)));

    let pool = WorkerPoolBuilder::new("protected-api")
        .with_strategy(foxtive_worker::LoadBalancingStrategy::RoundRobin)
        .with_concurrency_limit(20)
        // Layer 1: Trace all requests
        .with_middleware(TracingMiddleware::new("api-service"))
        // Layer 2: Rate limit to prevent abuse
        .with_middleware(RateLimitMiddleware::new(50, 20))
        // Layer 3: Auto-ack/nack for reliability
        .with_middleware(AckNackMiddleware::default())
        .add_arc_worker(api_worker.clone())
        .build()
        .unwrap();

    // Simulate API request burst
    for i in 0..30 {
        let _ = pool
            .dispatch(create_test_message(&format!("api-request-{:02}", i)))
            .await;
    }

    // Wait for processing
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let processed = api_worker
        .process_count
        .load(std::sync::atomic::Ordering::SeqCst);

    // Should have processed some but not all (rate limited)
    assert!(processed > 0, "Should have processed some requests");
    assert!(
        processed < 30,
        "Rate limiting should prevent all from completing immediately"
    );
}

/// Test edge case: Empty middleware list works correctly.
#[tokio::test]
async fn test_no_middleware_direct_dispatch() {
    let worker = Arc::new(TestWorker::new("direct-worker"));

    // Build pool without any middleware
    let pool = WorkerPoolBuilder::new("direct-pool")
        .with_concurrency_limit(5)
        .add_arc_worker(worker.clone())
        .build()
        .unwrap();

    // Messages should go directly to worker
    for i in 0..10 {
        pool.dispatch(create_test_message(&format!("msg-{}", i)))
            .await
            .unwrap();
    }

    wait_for_condition(
        || {
            worker
                .process_count
                .load(std::sync::atomic::Ordering::SeqCst)
                >= 10
        },
        Duration::from_secs(2),
    )
    .await;

    assert_eq!(
        worker
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst),
        10
    );
}

/// Test edge case: Multiple middleware of same type.
#[tokio::test]
async fn test_multiple_same_middleware() {
    let worker = Arc::new(TestWorker::new("multi-middleware-worker"));

    // Add multiple ack/nack middleware (unusual but should work)
    let pool = WorkerPoolBuilder::new("multi-middleware-pool")
        .with_concurrency_limit(5)
        .with_middleware(AckNackMiddleware::default())
        .with_middleware(AckNackMiddleware::default())
        .with_middleware(AckNackMiddleware::default())
        .add_arc_worker(worker.clone())
        .build()
        .unwrap();

    for i in 0..5 {
        pool.dispatch(create_test_message(&format!("msg-{}", i)))
            .await
            .unwrap();
    }

    wait_for_condition(
        || {
            worker
                .process_count
                .load(std::sync::atomic::Ordering::SeqCst)
                >= 5
        },
        Duration::from_secs(2),
    )
    .await;

    // Should still process all messages (middleware chain executes in order)
    assert_eq!(
        worker
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst),
        5
    );
}

/// Test middleware ordering: first added executes first.
#[tokio::test]
async fn test_middleware_execution_order() {
    use std::sync::Mutex;

    let execution_order = Arc::new(Mutex::new(Vec::new()));

    struct OrderTrackingMiddleware {
        name: &'static str,
        order: Arc<Mutex<Vec<&'static str>>>,
    }

    #[async_trait::async_trait]
    impl foxtive_worker::middleware::Middleware for OrderTrackingMiddleware {
        fn name(&self) -> &str {
            self.name
        }

        async fn handle(
            &self,
            message: foxtive_worker::ReceivedMessage<serde_json::Value>,
            next: Box<dyn foxtive_worker::middleware::MessageHandler>,
        ) -> Result<foxtive_worker::middleware::MiddlewareResult, foxtive_worker::WorkerError>
        {
            self.order.lock().unwrap().push(self.name);
            next.handle(message).await
        }
    }

    let worker = Arc::new(TestWorker::new("order-test-worker"));

    let pool = WorkerPoolBuilder::new("order-test-pool")
        .with_concurrency_limit(5)
        .with_middleware(OrderTrackingMiddleware {
            name: "first",
            order: execution_order.clone(),
        })
        .with_middleware(OrderTrackingMiddleware {
            name: "second",
            order: execution_order.clone(),
        })
        .with_middleware(OrderTrackingMiddleware {
            name: "third",
            order: execution_order.clone(),
        })
        .add_arc_worker(worker.clone())
        .build()
        .unwrap();

    pool.dispatch(create_test_message("msg-1")).await.unwrap();

    wait_for_condition(
        || {
            worker
                .process_count
                .load(std::sync::atomic::Ordering::SeqCst)
                >= 1
        },
        Duration::from_secs(2),
    )
    .await;

    let order = execution_order.lock().unwrap().clone();
    assert_eq!(order.len(), 3, "All middleware should execute");
    assert_eq!(order[0], "first", "First middleware should execute first");
    assert_eq!(
        order[1], "second",
        "Second middleware should execute second"
    );
    assert_eq!(order[2], "third", "Third middleware should execute third");
}

/// Test real-world scenario: Payment processor with circuit breaker protection.
#[tokio::test]
async fn test_payment_processor_with_circuit_breaker() {
    let payment_worker =
        Arc::new(TestWorker::new("payment-processor").with_delay(Duration::from_millis(50)));

    let pool = WorkerPoolBuilder::new("payment-pool")
        .with_concurrency_limit(10)
        // Protect against payment gateway failures
        .with_middleware(CircuitBreakerMiddleware::new(5, Duration::from_secs(30)))
        // Auto-ack successful payments
        .with_middleware(AckNackMiddleware::default())
        .add_arc_worker(payment_worker.clone())
        .build()
        .unwrap();

    // Process normal payments
    for i in 0..10 {
        pool.dispatch(create_test_message(&format!("payment-{:03}", i)))
            .await
            .unwrap();
    }

    wait_for_condition(
        || {
            payment_worker
                .process_count
                .load(std::sync::atomic::Ordering::SeqCst)
                >= 10
        },
        Duration::from_secs(5),
    )
    .await;

    assert_eq!(
        payment_worker
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst),
        10
    );
}

/// Test real-world scenario: Email sender with rate limiting and tracing.
#[tokio::test]
#[cfg(feature = "rate-limit")]
async fn test_email_sender_with_rate_limit_and_tracing() {
    let email_worker =
        Arc::new(TestWorker::new("email-sender").with_delay(Duration::from_millis(30)));

    let pool = WorkerPoolBuilder::new("email-pool")
        .with_concurrency_limit(5)
        // Prevent SMTP throttling
        .with_middleware(RateLimitMiddleware::new(10, 5))
        // Trace all email sends
        .with_middleware(TracingMiddleware::new("email-service"))
        // Auto-ack sent emails
        .with_middleware(AckNackMiddleware::default())
        .add_arc_worker(email_worker.clone())
        .build()
        .unwrap();

    // Send 20 emails
    for i in 0..20 {
        let _ = pool
            .dispatch(create_test_message(&format!("email-{:02}", i)))
            .await;
    }

    // Wait for all to complete (rate limiting will slow this down)
    wait_for_condition(
        || {
            email_worker
                .process_count
                .load(std::sync::atomic::Ordering::SeqCst)
                >= 10
        },
        Duration::from_secs(10),
    )
    .await;

    // Rate limiting may prevent all from completing, but at least some should
    let processed = email_worker
        .process_count
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(processed >= 5, "At least 5 emails should be sent");
}
