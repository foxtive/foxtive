mod common;
use common::*;
use foxtive_supervisor::Supervisor;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;

#[tokio::test]
async fn test_global_concurrency_limit() {
    let _started_count = Arc::new(AtomicUsize::new(0));

    let mut supervisor = Supervisor::new()
        .with_global_concurrency_limit(2);

    for i in 0..5 {
        let _count = _started_count.clone(); // Renamed to _count
        supervisor = supervisor.add(MockTask::new(&format!("task_{}", i)).with_backoff(foxtive_supervisor::enums::BackoffStrategy::Fixed(Duration::from_millis(100))));
    }

    // We need a custom task to track concurrent runs
    #[allow(dead_code)]
    struct ConcurrentTask {
        id: &'static str,
        counter: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for ConcurrentTask {
        fn id(&self) -> &'static str { self.id }
        async fn run(&self) -> anyhow::Result<()> {
            self.counter.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(100)).await;
            self.counter.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        }
    }

    let current_running = Arc::new(AtomicUsize::new(0));
    let max_running = Arc::new(AtomicUsize::new(0));
    let _max_cloned = max_running.clone(); // Renamed to _max_cloned

    struct TrackingTask {
        id: String,
        current: Arc<AtomicUsize>,
        max: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for TrackingTask {
        fn id(&self) -> &'static str { Box::leak(self.id.clone().into_boxed_str()) }
        async fn run(&self) -> anyhow::Result<()> {
            let val = self.current.fetch_add(1, Ordering::SeqCst) + 1;
            let mut m = self.max.load(Ordering::SeqCst);
            while val > m {
                match self.max.compare_exchange(m, val, Ordering::SeqCst, Ordering::SeqCst) {
                    Ok(_) => break,
                    Err(actual) => m = actual,
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            self.current.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        }
    }

    let mut supervisor = Supervisor::new().with_global_concurrency_limit(2);
    for i in 0..10 {
        supervisor = supervisor.add(TrackingTask {
            id: format!("task_{}", i),
            current: current_running.clone(),
            max: max_running.clone(),
        });
    }

    supervisor.start_and_wait_all().await.unwrap();

    assert!(max_running.load(Ordering::SeqCst) <= 2);
    assert!(max_running.load(Ordering::SeqCst) > 0);
}

#[tokio::test]
async fn test_priority_scheduling() {
    let execution_order = Arc::new(Mutex::new(Vec::new()));

    let mut supervisor = Supervisor::new()
        .with_global_concurrency_limit(1);

    struct PriorityTask {
        id: &'static str,
        priority: i32,
        order: Arc<Mutex<Vec<&'static str>>>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for PriorityTask {
        fn id(&self) -> &'static str { self.id }
        fn priority(&self) -> i32 { self.priority }
        async fn run(&self) -> anyhow::Result<()> {
            let mut o = self.order.lock().await;
            o.push(self.id);
            drop(o);
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    supervisor = supervisor
        .add(PriorityTask { id: "low", priority: 0, order: execution_order.clone() })
        .add(PriorityTask { id: "high", priority: 10, order: execution_order.clone() })
        .add(PriorityTask { id: "medium", priority: 5, order: execution_order.clone() });

    supervisor.start_and_wait_all().await.unwrap();

    let order = execution_order.lock().await;
    assert_eq!(order[0], "high");
    assert_eq!(order[1], "medium");
    assert_eq!(order[2], "low");
}
