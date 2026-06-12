mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::{TestWorker, create_test_message};
use foxtive_supervisor::enums::{BackoffStrategy, RestartPolicy};
use foxtive_supervisor::{SupervisedTask, Supervisor};
use foxtive_worker::{WorkerPool, WorkerPoolBuilder};

/// Wrapper that makes a WorkerPool compatible with foxtive-supervisor.
struct SupervisedWorkerPool {
    name: &'static str,
    pool: Arc<WorkerPool>,
}

impl SupervisedWorkerPool {
    fn new(name: impl Into<String>, pool: WorkerPool) -> Self {
        // Leak the string once during construction to get a 'static reference
        let static_name = Box::leak(name.into().into_boxed_str());
        Self {
            name: static_name,
            pool: Arc::new(pool),
        }
    }
}

#[async_trait::async_trait]
impl SupervisedTask for SupervisedWorkerPool {
    fn id(&self) -> &'static str {
        self.name
    }

    async fn run(&self) -> anyhow::Result<()> {
        // In a real scenario, this would consume messages from a backend
        // For testing, we just keep the task alive
        tokio::time::sleep(Duration::from_secs(1)).await;
        Ok(())
    }

    async fn cleanup(&self) {
        let _ = self.pool.shutdown().await;
    }
}

/// Test basic supervisor integration with worker pool.
#[tokio::test]
async fn test_supervisor_with_worker_pool() {
    let worker = Arc::new(TestWorker::new("supervised-worker"));

    let pool = WorkerPoolBuilder::new("supervised-pool")
        .with_concurrency_limit(5)
        .add_arc_worker(worker.clone())
        .build()
        .unwrap();

    let supervised_pool = SupervisedWorkerPool::new("worker-pool", pool);

    // Create supervisor and add the worker pool as a task
    let supervisor = Supervisor::new().add(supervised_pool);

    // Start supervisor (non-blocking)
    let runtime = supervisor.start().await.unwrap();

    // Give it time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Shutdown gracefully
    runtime.shutdown().await;
}

/// Test supervisor restarts worker pool on failure.
#[tokio::test]
async fn test_supervisor_restart_on_failure() {
    struct FailingPool {
        restarts: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl SupervisedTask for FailingPool {
        fn id(&self) -> &'static str {
            "failing-pool"
        }

        async fn run(&self) -> anyhow::Result<()> {
            self.restarts.fetch_add(1, Ordering::SeqCst);
            // Fail immediately
            Err(anyhow::anyhow!("Intentional failure"))
        }

        fn restart_policy(&self) -> RestartPolicy {
            RestartPolicy::Always
        }

        fn backoff_strategy(&self) -> BackoffStrategy {
            BackoffStrategy::fixed(Duration::from_millis(50))
        }
    }

    let restart_count = Arc::new(AtomicUsize::new(0));
    let restart_clone = restart_count.clone();

    let supervisor = Supervisor::new().add(FailingPool {
        restarts: restart_clone,
    });

    let runtime = supervisor.start().await.unwrap();

    // Wait for restarts to occur
    tokio::time::sleep(Duration::from_millis(300)).await;

    let restarts = restart_count.load(Ordering::SeqCst);
    assert!(
        restarts >= 2,
        "Expected at least 2 restarts, got {}",
        restarts
    );
    assert!(
        restarts <= 6,
        "Expected at most 6 restarts, got {}",
        restarts
    );

    runtime.shutdown().await;
}

/// Test multiple worker pools under single supervisor.
#[tokio::test]
async fn test_multiple_pools_under_supervisor() {
    let worker1 = Arc::new(TestWorker::new("pool1-worker"));
    let worker2 = Arc::new(TestWorker::new("pool2-worker"));

    let pool1 = WorkerPoolBuilder::new("email-pool")
        .with_concurrency_limit(3)
        .add_arc_worker(worker1.clone())
        .build()
        .unwrap();

    let pool2 = WorkerPoolBuilder::new("payment-pool")
        .with_concurrency_limit(5)
        .add_arc_worker(worker2.clone())
        .build()
        .unwrap();

    let supervised_pool1 = SupervisedWorkerPool::new("email-pool-task", pool1);
    let supervised_pool2 = SupervisedWorkerPool::new("payment-pool-task", pool2);

    let supervisor = Supervisor::new()
        .add(supervised_pool1)
        .add(supervised_pool2);

    let runtime = supervisor.start().await.unwrap();

    // Both pools should be running
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Shutdown
    runtime.shutdown().await;
}

/// Test graceful shutdown cascading to worker pools.
#[tokio::test]
async fn test_graceful_shutdown_cascade() {
    let worker =
        Arc::new(TestWorker::new("shutdown-test-worker").with_delay(Duration::from_millis(100)));

    let pool = WorkerPoolBuilder::new("cascade-pool")
        .with_concurrency_limit(3)
        .add_arc_worker(worker.clone())
        .build()
        .unwrap();

    // Dispatch some long-running messages
    for i in 0..5 {
        pool.dispatch(create_test_message(&format!("msg-{}", i)))
            .await
            .unwrap();
    }

    let supervised_pool = SupervisedWorkerPool::new("cascade-pool-task", pool);

    let supervisor = Supervisor::new().add(supervised_pool);

    let runtime = supervisor.start().await.unwrap();

    // Let messages start processing
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Initiate graceful shutdown
    let shutdown_start = std::time::Instant::now();
    runtime.shutdown().await;
    let shutdown_duration = shutdown_start.elapsed();

    // Shutdown should complete (may wait for in-flight messages)
    assert!(
        shutdown_duration < Duration::from_secs(5),
        "Shutdown took too long: {:?}",
        shutdown_duration
    );
}

/// Test supervisor with worker pool that has middleware.
#[tokio::test]
async fn test_supervisor_with_middleware_pool() {
    use foxtive_worker::{AckNackMiddleware, TracingMiddleware};

    let worker = Arc::new(TestWorker::new("middleware-supervised-worker"));

    let pool = WorkerPoolBuilder::new("middleware-pool")
        .with_concurrency_limit(5)
        .with_middleware(TracingMiddleware::new("supervised-service"))
        .with_middleware(AckNackMiddleware::default())
        .add_arc_worker(worker.clone())
        .build()
        .unwrap();

    let supervised_pool = SupervisedWorkerPool::new("middleware-pool-task", pool);

    let supervisor = Supervisor::new().add(supervised_pool);

    let runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    runtime.shutdown().await;
}

/// Test real-world scenario: Microservice with multiple supervised worker pools.
#[tokio::test]
async fn test_microservice_scenario() {
    // Simulate a microservice with different worker types
    let email_worker =
        Arc::new(TestWorker::new("email-worker").with_delay(Duration::from_millis(50)));
    let payment_worker =
        Arc::new(TestWorker::new("payment-worker").with_delay(Duration::from_millis(100)));
    let notification_worker =
        Arc::new(TestWorker::new("notification-worker").with_delay(Duration::from_millis(30)));

    // Create specialized pools
    let email_pool = WorkerPoolBuilder::new("email-pool")
        .with_concurrency_limit(3)
        .add_arc_worker(email_worker.clone())
        .build()
        .unwrap();

    let payment_pool = WorkerPoolBuilder::new("payment-pool")
        .with_concurrency_limit(2) // Strict limit for payments
        .add_arc_worker(payment_worker.clone())
        .build()
        .unwrap();

    let notification_pool = WorkerPoolBuilder::new("notification-pool")
        .with_concurrency_limit(10) // High concurrency for notifications
        .add_arc_worker(notification_worker.clone())
        .build()
        .unwrap();

    // Wrap for supervision
    let supervised_email = SupervisedWorkerPool::new("email-service", email_pool);
    let supervised_payment = SupervisedWorkerPool::new("payment-service", payment_pool);
    let supervised_notification =
        SupervisedWorkerPool::new("notification-service", notification_pool);

    // Create supervisor with all services
    let supervisor = Supervisor::new()
        .add(supervised_email)
        .add(supervised_payment)
        .add(supervised_notification);

    let runtime = supervisor.start().await.unwrap();

    // All services should be running
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Simulate service operation
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Graceful shutdown
    runtime.shutdown().await;
}

/// Test supervisor recovery when worker pool panics.
#[tokio::test]
async fn test_supervisor_recovers_from_panic() {
    struct PanickingPool {
        panics: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl SupervisedTask for PanickingPool {
        fn id(&self) -> &'static str {
            "panicking-pool"
        }

        async fn run(&self) -> anyhow::Result<()> {
            let count = self.panics.fetch_add(1, Ordering::SeqCst);
            if count == 0 {
                // Panic on first run
                panic!("Intentional panic for testing");
            }
            // Succeed on subsequent runs
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(())
        }

        fn restart_policy(&self) -> RestartPolicy {
            RestartPolicy::Always
        }

        fn backoff_strategy(&self) -> BackoffStrategy {
            BackoffStrategy::fixed(Duration::from_millis(50))
        }
    }

    let panic_count = Arc::new(AtomicUsize::new(0));
    let panic_clone = panic_count.clone();

    let supervisor = Supervisor::new().add(PanickingPool {
        panics: panic_clone,
    });

    let runtime = supervisor.start().await.unwrap();

    // Wait for panic and recovery
    tokio::time::sleep(Duration::from_millis(300)).await;

    let panics = panic_count.load(Ordering::SeqCst);
    assert!(panics >= 1, "Expected at least 1 panic, got {}", panics);

    runtime.shutdown().await;
}

/// Test supervisor with priority-based task ordering.
#[tokio::test]
async fn test_priority_based_ordering() {
    struct PriorityPool {
        name: &'static str,
        priority: i32,
    }

    #[async_trait::async_trait]
    impl SupervisedTask for PriorityPool {
        fn id(&self) -> &'static str {
            self.name
        }

        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(())
        }

        fn priority(&self) -> i32 {
            self.priority
        }
    }

    let low_priority = PriorityPool {
        name: "low-priority",
        priority: 1,
    };
    let high_priority = PriorityPool {
        name: "high-priority",
        priority: 10,
    };
    let medium_priority = PriorityPool {
        name: "medium-priority",
        priority: 5,
    };

    let supervisor = Supervisor::new()
        .add(low_priority)
        .add(high_priority)
        .add(medium_priority);

    let runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    runtime.shutdown().await;
}

/// Test supervisor with global concurrency limit.
#[tokio::test]
async fn test_global_concurrency_limit() {
    let worker1 =
        Arc::new(TestWorker::new("concurrent-worker-1").with_delay(Duration::from_millis(100)));
    let worker2 =
        Arc::new(TestWorker::new("concurrent-worker-2").with_delay(Duration::from_millis(100)));
    let worker3 =
        Arc::new(TestWorker::new("concurrent-worker-3").with_delay(Duration::from_millis(100)));

    let pool1 = WorkerPoolBuilder::new("pool-1")
        .with_concurrency_limit(1)
        .add_arc_worker(worker1.clone())
        .build()
        .unwrap();

    let pool2 = WorkerPoolBuilder::new("pool-2")
        .with_concurrency_limit(1)
        .add_arc_worker(worker2.clone())
        .build()
        .unwrap();

    let pool3 = WorkerPoolBuilder::new("pool-3")
        .with_concurrency_limit(1)
        .add_arc_worker(worker3.clone())
        .build()
        .unwrap();

    let supervised1 = SupervisedWorkerPool::new("supervised-pool-1", pool1);
    let supervised2 = SupervisedWorkerPool::new("supervised-pool-2", pool2);
    let supervised3 = SupervisedWorkerPool::new("supervised-pool-3", pool3);

    // Limit to only 2 concurrent tasks globally
    let supervisor = Supervisor::new()
        .with_global_concurrency_limit(2)
        .add(supervised1)
        .add(supervised2)
        .add(supervised3);

    let runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    runtime.shutdown().await;
}
