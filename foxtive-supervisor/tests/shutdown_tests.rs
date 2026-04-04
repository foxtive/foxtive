mod common;
use foxtive_supervisor::Supervisor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;

#[tokio::test]
async fn test_shutdown_order_respects_dependencies() {
    let shutdown_sequence = Arc::new(Mutex::new(Vec::new()));

    struct OrderedTask {
        id: &'static str,
        deps: &'static [&'static str],
        sequence: Arc<Mutex<Vec<&'static str>>>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for OrderedTask {
        fn id(&self) -> &'static str { self.id }
        fn dependencies(&self) -> &'static [&'static str] { self.deps }
        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            Ok(())
        }
        async fn on_shutdown(&self) {
            let mut seq = self.sequence.lock().await;
            seq.push(self.id);
        }
    }

    let supervisor = Supervisor::new()
        .add(OrderedTask { id: "db", deps: &[], sequence: shutdown_sequence.clone() })
        .add(OrderedTask { id: "api", deps: &["db"], sequence: shutdown_sequence.clone() })
        .add(OrderedTask { id: "worker", deps: &["db"], sequence: shutdown_sequence.clone() });

    let runtime = supervisor.start().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    runtime.shutdown().await;

    let seq = shutdown_sequence.lock().await;
    // api and worker should be before db
    let api_idx = seq.iter().position(|&id| id == "api").unwrap();
    let worker_idx = seq.iter().position(|&id| id == "worker").unwrap();
    let db_idx = seq.iter().position(|&id| id == "db").unwrap();

    assert!(api_idx < db_idx);
    assert!(worker_idx < db_idx);
}

#[tokio::test]
async fn test_shutdown_timeout_forces_termination() {
    struct StubbornTask {
        cleanup_started: Arc<AtomicBool>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for StubbornTask {
        fn id(&self) -> &'static str { "stubborn" }
        fn shutdown_timeout(&self) -> Duration { Duration::from_millis(100) }
        async fn run(&self) -> anyhow::Result<()> {
            // Ignore stop signal by sleeping in a way that doesn't check for cancellation
            // Actually, run() is aborted, but on_shutdown() is called.
            // If on_shutdown() hangs, the timeout should kick in.
            tokio::time::sleep(Duration::from_secs(3600)).await;
            Ok(())
        }
        async fn on_shutdown(&self) {
            self.cleanup_started.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(10)).await; // Hang here
        }
    }

    let cleanup_started = Arc::new(AtomicBool::new(false));
    let supervisor = Supervisor::new().add(StubbornTask { cleanup_started: cleanup_started.clone() });

    let runtime = supervisor.start().await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let start = std::time::Instant::now();
    runtime.shutdown().await;
    let elapsed = start.elapsed();

    assert!(cleanup_started.load(Ordering::SeqCst));
    // Should have taken roughly the timeout (100ms) plus some overhead, NOT 10 seconds.
    assert!(elapsed < Duration::from_secs(1));
}

#[tokio::test]
async fn test_shutdown_without_tasks() {
    let supervisor = Supervisor::new();
    let runtime = supervisor.start().await.unwrap();
    runtime.shutdown().await;
}

#[tokio::test]
async fn test_shutdown_with_mock_task() {
    let mut harness = common::testing::TestHarness::new();
    let mock = harness.add_mock("task1");
    let runtime = harness.supervisor.take().unwrap().start().await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    runtime.shutdown().await;
    assert!(mock.shutdown_called());
}
