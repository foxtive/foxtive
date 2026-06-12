mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{TestWorker, create_test_message, wait_for_condition};
use foxtive_worker::metrics::NoOpMetrics;
use foxtive_worker::{LoadBalancingStrategy, WorkerPool, WorkerPoolBuilder};

/// Test that pool handles rapid message bursts correctly.
#[tokio::test]
async fn test_rapid_message_burst() {
    let worker = Arc::new(TestWorker::new("burst-worker").with_delay(Duration::from_millis(10)));

    let mut pool = WorkerPool::with_concurrency(
        "burst-pool",
        LoadBalancingStrategy::RoundRobin,
        10,
        Arc::new(NoOpMetrics),
    );
    pool.add_worker(worker.clone());

    // Rapidly dispatch 100 messages
    let start = std::time::Instant::now();
    for i in 0..100 {
        pool.dispatch(create_test_message(&format!("burst-{:03}", i)))
            .await
            .unwrap();
    }
    let dispatch_time = start.elapsed();

    // Dispatch should be fast (non-blocking)
    assert!(
        dispatch_time < Duration::from_secs(1),
        "Dispatch should be non-blocking"
    );

    // Wait for all to complete
    wait_for_condition(
        || {
            worker
                .process_count
                .load(std::sync::atomic::Ordering::SeqCst)
                >= 100
        },
        Duration::from_secs(10),
    )
    .await;

    assert_eq!(
        worker
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst),
        100
    );
}

/// Test graceful handling of worker failures.
#[tokio::test]
async fn test_worker_failure_handling() {
    let worker = Arc::new(TestWorker::new("failing-worker"));

    let mut pool = WorkerPool::with_concurrency(
        "failure-pool",
        LoadBalancingStrategy::RoundRobin,
        5,
        Arc::new(NoOpMetrics),
    );
    pool.add_worker(worker.clone());

    // Make worker fail for some messages
    worker.set_should_fail(true);

    for i in 0..10 {
        pool.dispatch(create_test_message(&format!("fail-msg-{}", i)))
            .await
            .unwrap();
    }

    // Wait for processing attempts
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Messages should still be "processed" (even though they failed)
    let processed = worker
        .process_count
        .load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        processed, 10,
        "Worker should attempt all messages even if failing"
    );
}

/// Test load balancing distributes messages evenly.
#[tokio::test]
async fn test_round_robin_distribution() {
    let worker1 = Arc::new(TestWorker::new("rr-worker-1").with_delay(Duration::from_millis(20)));
    let worker2 = Arc::new(TestWorker::new("rr-worker-2").with_delay(Duration::from_millis(20)));
    let worker3 = Arc::new(TestWorker::new("rr-worker-3").with_delay(Duration::from_millis(20)));

    let mut pool = WorkerPool::with_concurrency(
        "rr-pool",
        LoadBalancingStrategy::RoundRobin,
        20,
        Arc::new(NoOpMetrics),
    );
    pool.add_worker(worker1.clone());
    pool.add_worker(worker2.clone());
    pool.add_worker(worker3.clone());

    // Dispatch 30 messages
    for i in 0..30 {
        pool.dispatch(create_test_message(&format!("msg-{:02}", i)))
            .await
            .unwrap();
    }

    // Wait for completion
    wait_for_condition(
        || {
            let total = worker1
                .process_count
                .load(std::sync::atomic::Ordering::SeqCst)
                + worker2
                    .process_count
                    .load(std::sync::atomic::Ordering::SeqCst)
                + worker3
                    .process_count
                    .load(std::sync::atomic::Ordering::SeqCst);
            total >= 30
        },
        Duration::from_secs(5),
    )
    .await;

    let count1 = worker1
        .process_count
        .load(std::sync::atomic::Ordering::SeqCst);
    let count2 = worker2
        .process_count
        .load(std::sync::atomic::Ordering::SeqCst);
    let count3 = worker3
        .process_count
        .load(std::sync::atomic::Ordering::SeqCst);

    // Each worker should have processed approximately 10 messages
    assert!(
        (8..=12).contains(&count1),
        "Worker1 got {}, expected ~10",
        count1
    );
    assert!(
        (8..=12).contains(&count2),
        "Worker2 got {}, expected ~10",
        count2
    );
    assert!(
        (8..=12).contains(&count3),
        "Worker3 got {}, expected ~10",
        count3
    );
}

/// Test least-loaded balancing prefers idle workers.
#[tokio::test]
async fn test_least_loaded_balancing() {
    let worker1 = Arc::new(TestWorker::new("ll-worker-1").with_delay(Duration::from_millis(100)));
    let worker2 = Arc::new(TestWorker::new("ll-worker-2").with_delay(Duration::from_millis(10)));

    let mut pool = WorkerPool::with_concurrency(
        "ll-pool",
        LoadBalancingStrategy::LeastLoaded,
        10,
        Arc::new(NoOpMetrics),
    );
    pool.add_worker(worker1.clone());
    pool.add_worker(worker2.clone());

    // Dispatch messages - faster worker should get more due to completing faster
    for i in 0..20 {
        pool.dispatch(create_test_message(&format!("msg-{:02}", i)))
            .await
            .unwrap();
    }

    // Wait for completion
    wait_for_condition(
        || {
            let total = worker1
                .process_count
                .load(std::sync::atomic::Ordering::SeqCst)
                + worker2
                    .process_count
                    .load(std::sync::atomic::Ordering::SeqCst);
            total >= 20
        },
        Duration::from_secs(5),
    )
    .await;

    let count1 = worker1
        .process_count
        .load(std::sync::atomic::Ordering::SeqCst);
    let count2 = worker2
        .process_count
        .load(std::sync::atomic::Ordering::SeqCst);

    // Both workers should have processed messages (load balancing works)
    assert_eq!(count1 + count2, 20, "All messages should be processed");
    assert!(
        count1 > 0 && count2 > 0,
        "Both workers should process some messages"
    );
}

/// Test pool shutdown prevents new dispatches.
#[tokio::test]
async fn test_pool_shutdown() {
    let worker =
        Arc::new(TestWorker::new("shutdown-worker").with_delay(Duration::from_millis(100)));

    let mut pool = WorkerPool::with_concurrency(
        "shutdown-pool",
        LoadBalancingStrategy::RoundRobin,
        5,
        Arc::new(NoOpMetrics),
    );
    pool.add_worker(worker.clone());

    // Dispatch some messages
    for i in 0..3 {
        pool.dispatch(create_test_message(&format!("msg-{}", i)))
            .await
            .unwrap();
    }

    // Shutdown the pool
    pool.shutdown().await.unwrap();

    // Try to dispatch after shutdown - should fail or hang
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        pool.dispatch(create_test_message("after-shutdown")),
    )
    .await;

    // Should timeout or error
    assert!(
        result.is_err() || result.unwrap().is_err(),
        "Should not dispatch after shutdown"
    );
}

/// Test empty pool rejects messages.
#[tokio::test]
async fn test_empty_pool_rejection() {
    let pool = WorkerPool::new(
        "empty-pool",
        LoadBalancingStrategy::RoundRobin,
        Arc::new(NoOpMetrics),
    );

    let result = pool.dispatch(create_test_message("msg-1")).await;

    assert!(result.is_err(), "Empty pool should reject messages");

    if let Err(e) = result {
        assert!(
            e.to_string().contains("exhausted") || e.to_string().contains("empty"),
            "Error should indicate pool is exhausted: {}",
            e
        );
    }
}

/// Test concurrent access to same pool from multiple tasks.
#[tokio::test]
async fn test_concurrent_pool_access() {
    let worker =
        Arc::new(TestWorker::new("concurrent-worker").with_delay(Duration::from_millis(10)));

    let pool = Arc::new(
        WorkerPoolBuilder::new("concurrent-pool")
            .with_concurrency_limit(10)
            .add_arc_worker(worker.clone())
            .build()
            .unwrap(),
    );

    // Spawn 10 tasks that each dispatch 10 messages
    let mut handles = vec![];
    for task_id in 0..10 {
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            for i in 0..10 {
                let msg = create_test_message(&format!("task{}-msg{}", task_id, i));
                pool_clone.dispatch(msg).await.unwrap();
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Wait for all messages to be processed
    wait_for_condition(
        || {
            worker
                .process_count
                .load(std::sync::atomic::Ordering::SeqCst)
                >= 100
        },
        Duration::from_secs(10),
    )
    .await;

    assert_eq!(
        worker
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst),
        100
    );
}

/// Test real-world scenario: Background job processor with mixed workloads.
#[tokio::test]
async fn test_background_job_processor() {
    // Simulate a background job processor handling different types of jobs
    let fast_worker = Arc::new(TestWorker::new("fast-jobs").with_delay(Duration::from_millis(5)));
    let slow_worker = Arc::new(TestWorker::new("slow-jobs").with_delay(Duration::from_millis(100)));

    let mut pool = WorkerPool::with_concurrency(
        "job-processor",
        LoadBalancingStrategy::LeastLoaded,
        15,
        Arc::new(NoOpMetrics),
    );
    pool.add_worker(fast_worker.clone());
    pool.add_worker(slow_worker.clone());

    // Mix of fast and slow jobs
    for i in 0..50 {
        pool.dispatch(create_test_message(&format!("job-{:03}", i)))
            .await
            .unwrap();
    }

    // Wait for completion
    wait_for_condition(
        || {
            let total = fast_worker
                .process_count
                .load(std::sync::atomic::Ordering::SeqCst)
                + slow_worker
                    .process_count
                    .load(std::sync::atomic::Ordering::SeqCst);
            total >= 50
        },
        Duration::from_secs(10),
    )
    .await;

    let fast_count = fast_worker
        .process_count
        .load(std::sync::atomic::Ordering::SeqCst);
    let slow_count = slow_worker
        .process_count
        .load(std::sync::atomic::Ordering::SeqCst);

    assert_eq!(fast_count + slow_count, 50, "All jobs should be processed");
    // Both workers should have processed some messages
    assert!(
        fast_count > 0 && slow_count > 0,
        "Both workers should handle jobs"
    );
}

/// Test real-world scenario: Notification system with high throughput.
#[tokio::test]
async fn test_notification_system_high_throughput() {
    let notification_worker =
        Arc::new(TestWorker::new("notification-sender").with_delay(Duration::from_millis(5)));

    let pool = WorkerPoolBuilder::new("notification-pool")
        .with_strategy(LoadBalancingStrategy::RoundRobin)
        .with_concurrency_limit(50) // High concurrency for notifications
        .add_arc_worker(notification_worker.clone())
        .build()
        .unwrap();

    // Simulate burst of 200 notifications
    let start = std::time::Instant::now();
    for i in 0..200 {
        pool.dispatch(create_test_message(&format!("notif-{:04}", i)))
            .await
            .unwrap();
    }

    // Wait for all to complete
    wait_for_condition(
        || {
            notification_worker
                .process_count
                .load(std::sync::atomic::Ordering::SeqCst)
                >= 200
        },
        Duration::from_secs(10),
    )
    .await;
    let elapsed = start.elapsed();

    assert_eq!(
        notification_worker
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst),
        200
    );

    // With 50 concurrent and 5ms per notification, should complete in ~20ms + overhead
    assert!(
        elapsed < Duration::from_secs(2),
        "High throughput should be fast: {:?}",
        elapsed
    );
}

/// Test edge case: Very large number of messages.
#[tokio::test]
async fn test_large_message_volume() {
    let worker = Arc::new(TestWorker::new("volume-worker").with_delay(Duration::from_millis(1)));

    let mut pool = WorkerPool::with_concurrency(
        "volume-pool",
        LoadBalancingStrategy::RoundRobin,
        20,
        Arc::new(NoOpMetrics),
    );
    pool.add_worker(worker.clone());

    // Dispatch 500 messages
    for i in 0..500 {
        pool.dispatch(create_test_message(&format!("vol-{:04}", i)))
            .await
            .unwrap();
    }

    // Wait for completion
    wait_for_condition(
        || {
            worker
                .process_count
                .load(std::sync::atomic::Ordering::SeqCst)
                >= 500
        },
        Duration::from_secs(30),
    )
    .await;

    assert_eq!(
        worker
            .process_count
            .load(std::sync::atomic::Ordering::SeqCst),
        500
    );
}
