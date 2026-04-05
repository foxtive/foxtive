use anyhow::Result;
use async_trait::async_trait;
use foxtive_supervisor::task_pool::{LoadBalancingStrategy, TaskPool};
use foxtive_supervisor::{SupervisedTask};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time::{sleep, Duration};
use tracing::{info, Level};
use tracing_subscriber;

/// A worker that processes messages from a queue
struct MessageWorker {
    worker_id: usize,
    processed_count: Arc<AtomicUsize>,
}

impl MessageWorker {
    fn new(worker_id: usize, processed_count: Arc<AtomicUsize>) -> Self {
        Self {
            worker_id,
            processed_count,
        }
    }
}

#[async_trait]
impl SupervisedTask for MessageWorker {
    fn id(&self) -> &'static str {
        // Each worker has a unique ID
        Box::leak(format!("message-worker-{}", self.worker_id).into_boxed_str())
    }

    async fn setup(&self) -> Result<()> {
        info!(worker = self.worker_id, "Setting up message worker");
        sleep(Duration::from_millis(100)).await;
        Ok(())
    }

    async fn run(&self) -> Result<()> {
        // Simulate processing a message
        sleep(Duration::from_millis(500)).await;
        
        let count = self.processed_count.fetch_add(1, Ordering::SeqCst) + 1;
        info!(
            worker = self.worker_id,
            count,
            "Processed message"
        );
        
        Ok(())
    }

    async fn cleanup(&self) {
        info!(worker = self.worker_id, "Cleaning up message worker");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting task pools example");
    info!("This demonstrates load-balanced worker pools");

    let total_processed = Arc::new(AtomicUsize::new(0));

    // Example 1: Round-robin distribution
    info!("\n=== Round-Robin Pool ===");
    let round_robin_pool = TaskPool::new(
        "round-robin-pool",
        3, // 3 workers
        LoadBalancingStrategy::RoundRobin,
    );

    let rr_supervisor = round_robin_pool.build_pool(|worker_id| {
        MessageWorker::new(worker_id, total_processed.clone())
    });

    info!("Started round-robin pool with 3 workers");
    
    // Let it run briefly
    sleep(Duration::from_secs(4)).await;
    
    let rr_count = total_processed.load(Ordering::SeqCst);
    info!(rr_count, "Messages processed by round-robin pool");

    rr_supervisor.shutdown().await;
    info!("Round-robin pool stopped");

    // Reset counter
    total_processed.store(0, Ordering::SeqCst);

    // Example 2: Random distribution
    info!("\n=== Random Distribution Pool ===");
    let random_pool = TaskPool::new(
        "random-pool",
        4, // 4 workers
        LoadBalancingStrategy::Random,
    );

    let random_supervisor = random_pool.build_pool(|worker_id| {
        MessageWorker::new(worker_id, total_processed.clone())
    });

    info!("Started random pool with 4 workers");
    
    sleep(Duration::from_secs(4)).await;
    
    let random_count = total_processed.load(Ordering::SeqCst);
    info!(random_count, "Messages processed by random pool");

    random_supervisor.shutdown().await;
    info!("Random pool stopped");

    // Reset counter
    total_processed.store(0, Ordering::SeqCst);

    // Example 3: Least-loaded distribution
    info!("\n=== Least-Loaded Pool ===");
    let least_loaded_pool = TaskPool::new(
        "least-loaded-pool",
        3, // 3 workers
        LoadBalancingStrategy::LeastLoaded,
    );

    let ll_supervisor = least_loaded_pool.build_pool(|worker_id| {
        MessageWorker::new(worker_id, total_processed.clone())
    });

    info!("Started least-loaded pool with 3 workers");
    
    sleep(Duration::from_secs(4)).await;
    
    let ll_count = total_processed.load(Ordering::SeqCst);
    info!(ll_count, "Messages processed by least-loaded pool");

    ll_supervisor.shutdown().await;
    info!("Least-loaded pool stopped");

    info!("\n=== Summary ===");
    info!("Task pools provide:");
    info!("  - Automatic worker management");
    info!("  - Load balancing across multiple strategies");
    info!("  - Easy scaling by adjusting pool size");
    info!("  - Built-in fault tolerance with supervision");

    info!("\nExample completed successfully!");

    Ok(())
}
