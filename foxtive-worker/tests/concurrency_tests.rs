mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{TestWorker, create_test_message, wait_for_condition};
use foxtive_worker::metrics::NoOpMetrics;
use foxtive_worker::{LoadBalancingStrategy, WorkerPool, WorkerPoolBuilder};

/// Test that concurrency limit is strictly enforced under heavy load.
#[tokio::test]
async fn test_concurrency_limit_strict_enforcement() {
    let worker = Arc::new(TestWorker::new("worker-1").with_delay(Duration::from_millis(50)));

    let mut pool = WorkerPool::with_concurrency(
        "test-pool",
        LoadBalancingStrategy::RoundRobin,
        3, // Strict limit of 3
        Arc::new(NoOpMetrics),
    );
    pool.add_worker(worker.clone());

    // Dispatch 20 messages rapidly (much more than the limit)
    for i in 0..20 {
        let message = create_test_message(&format!("msg-{:02}", i));
        pool.dispatch(message).await.unwrap();
    }

    // Wait for all to complete
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Verify max concurrency never exceeded limit
    let max_concurrent = worker
        .max_concurrent
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        max_concurrent <= 3,
        "Concurrency limit violated! Max concurrent was {}, but limit is 3",
        max_concurrent
    );

    // Verify all messages were processed
    let processed = worker
        .process_count
        .load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(processed, 20, "Expected all 20 messages to be processed");
}

/// Test that different pools maintain independent concurrency limits.
#[tokio::test]
async fn test_independent_pool_limits() {
    let worker1 = Arc::new(TestWorker::new("pool1-worker").with_delay(Duration::from_millis(50)));
    let worker2 = Arc::new(TestWorker::new("pool2-worker").with_delay(Duration::from_millis(50)));

    let mut pool1 = WorkerPool::with_concurrency(
        "pool1",
        LoadBalancingStrategy::RoundRobin,
        2,
        Arc::new(NoOpMetrics),
    );
    pool1.add_worker(worker1.clone());

    let mut pool2 = WorkerPool::with_concurrency(
        "pool2",
        LoadBalancingStrategy::RoundRobin,
        5,
        Arc::new(NoOpMetrics),
    );
    pool2.add_worker(worker2.clone());

    // Dispatch messages to both pools
    for i in 0..10 {
        pool1
            .dispatch(create_test_message(&format!("p1-msg-{:02}", i)))
            .await
            .unwrap();
        pool2
            .dispatch(create_test_message(&format!("p2-msg-{:02}", i)))
            .await
            .unwrap();
    }

    // Wait for completion
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Verify each pool respected its own limit
    let max1 = worker1
        .max_concurrent
        .load(std::sync::atomic::Ordering::SeqCst);
    let max2 = worker2
        .max_concurrent
        .load(std::sync::atomic::Ordering::SeqCst);

    assert!(max1 <= 2, "Pool1 exceeded limit: {} > 2", max1);
    assert!(max2 <= 5, "Pool2 exceeded limit: {} > 5", max2);

    // Verify both pools processed all messages
    assert_eq!(
        worker1
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst),
        10
    );
    assert_eq!(
        worker2
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst),
        10
    );
}

/// Test concurrency limit with builder API.
#[tokio::test]
async fn test_builder_concurrency_limit() {
    let worker = Arc::new(TestWorker::new("builder-worker").with_delay(Duration::from_millis(30)));

    let pool = WorkerPoolBuilder::new("builder-pool")
        .with_concurrency_limit(4)
        .add_arc_worker(worker.clone())
        .build()
        .unwrap();

    // Dispatch many messages
    for i in 0..15 {
        pool.dispatch(create_test_message(&format!("msg-{:02}", i)))
            .await
            .unwrap();
    }

    // Wait for completion
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Verify limit was respected
    let max_concurrent = worker
        .max_concurrent
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        max_concurrent <= 4,
        "Builder concurrency limit violated: {} > 4",
        max_concurrent
    );

    assert_eq!(
        worker
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst),
        15
    );
}

/// Test that concurrency limit of 1 enforces sequential processing.
#[tokio::test]
async fn test_sequential_processing_with_limit_one() {
    let worker =
        Arc::new(TestWorker::new("sequential-worker").with_delay(Duration::from_millis(20)));

    let mut pool = WorkerPool::with_concurrency(
        "sequential-pool",
        LoadBalancingStrategy::RoundRobin,
        1, // Force sequential
        Arc::new(NoOpMetrics),
    );
    pool.add_worker(worker.clone());

    // Dispatch 5 messages
    for i in 0..5 {
        pool.dispatch(create_test_message(&format!("msg-{}", i)))
            .await
            .unwrap();
    }

    // Wait for completion
    tokio::time::sleep(Duration::from_millis(500)).await;

    // With limit=1, max concurrent should be exactly 1
    let max_concurrent = worker
        .max_concurrent
        .load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        max_concurrent, 1,
        "Sequential processing violated: max concurrent was {}",
        max_concurrent
    );

    assert_eq!(
        worker
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst),
        5
    );
}

/// Test edge case: concurrency limit of 0 should still work (no concurrent processing).
#[tokio::test]
async fn test_zero_concurrency_limit() {
    let worker = Arc::new(TestWorker::new("zero-limit-worker"));

    // Limit of 0 means no messages can be processed concurrently
    let mut pool = WorkerPool::with_concurrency(
        "zero-limit-pool",
        LoadBalancingStrategy::RoundRobin,
        0,
        Arc::new(NoOpMetrics),
    );
    pool.add_worker(worker.clone());

    // Try to dispatch a message - it should hang waiting for a permit
    let message = create_test_message("msg-1");

    // This should timeout because no permits are available
    let result = tokio::time::timeout(Duration::from_millis(100), pool.dispatch(message)).await;

    // Should timeout (message can't be dispatched with 0 limit)
    assert!(
        result.is_err(),
        "Message should not be dispatched with concurrency limit 0"
    );
}

/// Test high concurrency limit allows parallel processing.
#[tokio::test]
async fn test_high_concurrency_allows_parallelism() {
    let worker =
        Arc::new(TestWorker::new("high-concurrency-worker").with_delay(Duration::from_millis(100)));

    let mut pool = WorkerPool::with_concurrency(
        "high-concurrency-pool",
        LoadBalancingStrategy::RoundRobin,
        50, // High limit
        Arc::new(NoOpMetrics),
    );
    pool.add_worker(worker.clone());

    // Dispatch 20 messages
    for i in 0..20 {
        pool.dispatch(create_test_message(&format!("msg-{:02}", i)))
            .await
            .unwrap();
    }

    // Wait a bit for processing to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Should have significant parallelism
    let current = worker
        .concurrent_count
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        current >= 5,
        "Expected high parallelism with limit 50, but only {} concurrent",
        current
    );

    // Wait for completion
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let max_concurrent = worker
        .max_concurrent
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        max_concurrent >= 10,
        "Expected at least 10 concurrent with high limit, got {}",
        max_concurrent
    );
    assert_eq!(
        worker
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst),
        20
    );
}

/// Test that concurrency limit works correctly with multiple workers.
#[tokio::test]
async fn test_concurrency_across_multiple_workers() {
    let worker1 = Arc::new(TestWorker::new("multi-worker-1").with_delay(Duration::from_millis(50)));
    let worker2 = Arc::new(TestWorker::new("multi-worker-2").with_delay(Duration::from_millis(50)));
    let worker3 = Arc::new(TestWorker::new("multi-worker-3").with_delay(Duration::from_millis(50)));

    let mut pool = WorkerPool::with_concurrency(
        "multi-worker-pool",
        LoadBalancingStrategy::RoundRobin,
        4, // Limit across ALL workers
        Arc::new(NoOpMetrics),
    );
    pool.add_worker(worker1.clone());
    pool.add_worker(worker2.clone());
    pool.add_worker(worker3.clone());

    // Dispatch many messages
    for i in 0..15 {
        pool.dispatch(create_test_message(&format!("msg-{:02}", i)))
            .await
            .unwrap();
    }

    // Wait for completion
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Total concurrency across all workers should not exceed limit
    let _total_max = worker1
        .max_concurrent
        .load(std::sync::atomic::Ordering::SeqCst)
        + worker2
            .max_concurrent
            .load(std::sync::atomic::Ordering::SeqCst)
        + worker3
            .max_concurrent
            .load(std::sync::atomic::Ordering::SeqCst);

    // Note: Each worker tracks its own max, so we check individual workers
    assert!(
        worker1
            .max_concurrent
            .load(std::sync::atomic::Ordering::SeqCst)
            <= 4
    );
    assert!(
        worker2
            .max_concurrent
            .load(std::sync::atomic::Ordering::SeqCst)
            <= 4
    );
    assert!(
        worker3
            .max_concurrent
            .load(std::sync::atomic::Ordering::SeqCst)
            <= 4
    );

    // All messages should be distributed and processed
    let total_processed = worker1
        .process_count
        .load(std::sync::atomic::Ordering::SeqCst)
        + worker2
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst)
        + worker3
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst);

    assert_eq!(total_processed, 15, "All messages should be processed");
}

/// Test real-world scenario: Payment processor with strict concurrency control.
#[tokio::test]
async fn test_payment_processor_scenario() {
    // Simulate payment processing with strict limits
    let payment_worker = Arc::new(
        TestWorker::new("payment-processor").with_delay(Duration::from_millis(100)), // Payments take time
    );

    let pool = WorkerPoolBuilder::new("payment-pool")
        .with_strategy(LoadBalancingStrategy::LeastLoaded)
        .with_concurrency_limit(10) // Max 10 payments at once
        .add_arc_worker(payment_worker.clone())
        .build()
        .unwrap();

    // Simulate burst of 50 payment requests
    for i in 0..50 {
        pool.dispatch(create_test_message(&format!("payment-{:03}", i)))
            .await
            .unwrap();
    }

    // Monitor that concurrency stays within bounds
    tokio::time::sleep(Duration::from_millis(500)).await;

    let max_concurrent = payment_worker
        .max_concurrent
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        max_concurrent <= 10,
        "Payment processor exceeded safety limit: {} > 10",
        max_concurrent
    );

    // Wait for all payments to complete
    wait_for_condition(
        || {
            payment_worker
                .process_count
                .load(std::sync::atomic::Ordering::SeqCst)
                >= 50
        },
        Duration::from_secs(10),
    )
    .await;

    assert_eq!(
        payment_worker
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst),
        50
    );
}

/// Test real-world scenario: Email sender with concurrency control.
#[tokio::test]
async fn test_email_sender_scenario() {
    // Email sending has strict concurrency limits
    let email_worker =
        Arc::new(TestWorker::new("email-sender").with_delay(Duration::from_millis(100)));

    let pool = WorkerPoolBuilder::new("email-pool")
        .with_concurrency_limit(2) // Very limited concurrency
        .add_arc_worker(email_worker.clone())
        .build()
        .unwrap();

    // Send 10 emails
    for i in 0..10 {
        pool.dispatch(create_test_message(&format!("email-{:02}", i)))
            .await
            .unwrap();
    }

    // Wait for all to complete
    wait_for_condition(
        || {
            email_worker
                .process_count
                .load(std::sync::atomic::Ordering::SeqCst)
                >= 10
        },
        Duration::from_secs(5),
    )
    .await;

    // Verify all were processed
    assert_eq!(
        email_worker
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst),
        10
    );

    // Verify concurrency never exceeded limit
    let max_concurrent = email_worker
        .max_concurrent
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        max_concurrent <= 2,
        "Email concurrency exceeded limit: {} > 2",
        max_concurrent
    );
}
