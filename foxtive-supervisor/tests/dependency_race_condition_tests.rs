mod common;
use foxtive_supervisor::Supervisor;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Test that verifies the dependency signaling race condition is fixed.
/// This test creates a fast dependency that completes setup before the dependent
/// task subscribes. With watch channels, the dependent should still receive the signal.
#[tokio::test]
async fn test_dependency_race_condition_fix() {
    struct FastDependency {
        id: &'static str,
        setup_complete: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for FastDependency {
        fn id(&self) -> &'static str { self.id }
        
        async fn setup(&self) -> anyhow::Result<()> {
            // Complete setup immediately
            self.setup_complete.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            // Keep running
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(())
        }
    }

    struct SlowDependent {
        id: &'static str,
        deps: &'static [&'static str],
        started: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for SlowDependent {
        fn id(&self) -> &'static str { self.id }
        
        fn dependencies(&self) -> &'static [&'static str] {
            self.deps
        }
        
        async fn setup(&self) -> anyhow::Result<()> {
            // Simulate slow startup - dependency will complete before this
            tokio::time::sleep(Duration::from_millis(100)).await;
            self.started.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(())
        }
    }

    let fast_setup_count = Arc::new(AtomicUsize::new(0));
    let slow_started = Arc::new(AtomicUsize::new(0));

    let supervisor = Supervisor::new()
        .add(FastDependency {
            id: "fast-dep",
            setup_complete: fast_setup_count.clone(),
        })
        .add(SlowDependent {
            id: "slow-dependent",
            deps: &["fast-dep"],
            started: slow_started.clone(),
        });

    let runtime = supervisor.start().await.unwrap();
    
    // Give tasks time to complete setup and run
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Verify both tasks completed setup successfully
    // If race condition existed, slow-dependent would hang waiting for signal
    assert_eq!(fast_setup_count.load(Ordering::SeqCst), 1, "Fast dependency should complete setup");
    assert_eq!(slow_started.load(Ordering::SeqCst), 1, "Slow dependent should start despite late subscription");
    
    println!("✓ Dependency race condition fix verified");
    println!("  Fast dependency setup count: {}", fast_setup_count.load(Ordering::SeqCst));
    println!("  Slow dependent started count: {}", slow_started.load(Ordering::SeqCst));
    
    runtime.shutdown().await;
}

/// Test multiple dependents with varying startup times
#[tokio::test]
async fn test_multiple_dependents_race_condition() {
    struct QuickService {
        id: &'static str,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for QuickService {
        fn id(&self) -> &'static str { self.id }
        
        async fn setup(&self) -> anyhow::Result<()> {
            // Instant setup
            Ok(())
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(200)).await;
            Ok(())
        }
    }

    struct DelayedWorker {
        id: &'static str,
        delay_ms: u64,
        execution_order: Arc<std::sync::Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for DelayedWorker {
        fn id(&self) -> &'static str { self.id }
        
        fn dependencies(&self) -> &'static [&'static str] {
            &["quick-service"]
        }
        
        async fn setup(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            self.execution_order.lock().unwrap().push(self.id.to_string());
            Ok(())
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let execution_order = Arc::new(std::sync::Mutex::new(Vec::new()));

    let supervisor = Supervisor::new()
        .add(QuickService { id: "quick-service" })
        .add(DelayedWorker {
            id: "worker-1",
            delay_ms: 50,
            execution_order: execution_order.clone(),
        })
        .add(DelayedWorker {
            id: "worker-2",
            delay_ms: 100,
            execution_order: execution_order.clone(),
        })
        .add(DelayedWorker {
            id: "worker-3",
            delay_ms: 150,
            execution_order: execution_order.clone(),
        });

    let runtime = supervisor.start().await.unwrap();
    
    // Give all workers time to complete
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // All three workers should have started despite quick-service completing before they subscribed
    let order = execution_order.lock().unwrap();
    assert_eq!(order.len(), 3, "All three workers should have started");
    
    println!("✓ Multiple dependents race condition test passed");
    println!("  Workers started in order: {:?}", *order);
    
    runtime.shutdown().await;
}

/// Test dependency chain where each depends on the previous
#[tokio::test]
async fn test_dependency_chain_no_hang() {
    struct ChainLink {
        id: &'static str,
        deps: &'static [&'static str],
        link_number: usize,
        reached: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for ChainLink {
        fn id(&self) -> &'static str { self.id }
        
        fn dependencies(&self) -> &'static [&'static str] {
            self.deps
        }
        
        async fn setup(&self) -> anyhow::Result<()> {
            // Each link takes progressively longer
            tokio::time::sleep(Duration::from_millis(self.link_number as u64 * 10)).await;
            self.reached.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    let reached_count = Arc::new(AtomicUsize::new(0));

    let supervisor = Supervisor::new()
        .add(ChainLink {
            id: "link-1",
            deps: &[],
            link_number: 1,
            reached: reached_count.clone(),
        })
        .add(ChainLink {
            id: "link-2",
            deps: &["link-1"],
            link_number: 2,
            reached: reached_count.clone(),
        })
        .add(ChainLink {
            id: "link-3",
            deps: &["link-2"],
            link_number: 3,
            reached: reached_count.clone(),
        })
        .add(ChainLink {
            id: "link-4",
            deps: &["link-3"],
            link_number: 4,
            reached: reached_count.clone(),
        });

    let runtime = supervisor.start().await.unwrap();
    
    // Give chain time to propagate
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // All links should have been reached without hanging
    assert_eq!(reached_count.load(Ordering::SeqCst), 4, "All chain links should execute");
    
    println!("✓ Dependency chain test passed - no hangs detected");
    println!("  Chain links executed: {}", reached_count.load(Ordering::SeqCst));
    
    runtime.shutdown().await;
}
