use anyhow::Result;
use async_trait::async_trait;
use foxtive_supervisor::{persistence::FsStateStore, Supervisor, SupervisedTask};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time::{sleep, Duration};
use tracing::{info, Level};

/// A message processor that persists its state across restarts
struct MessageProcessor {
    processed_count: Arc<AtomicUsize>,
}

#[async_trait]
impl SupervisedTask for MessageProcessor {
    fn id(&self) -> &'static str {
        "message-processor"
    }

    fn backoff_strategy(&self) -> foxtive_supervisor::enums::BackoffStrategy {
        foxtive_supervisor::enums::BackoffStrategy::Exponential {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(10),
        }
    }

    async fn setup(&self) -> Result<()> {
        info!("Setting up message processor");
        // Simulate connection to message queue
        sleep(Duration::from_millis(500)).await;
        Ok(())
    }

    async fn run(&self) -> Result<()> {
        // Simulate processing a message
        sleep(Duration::from_millis(200)).await;
        
        let count = self.processed_count.fetch_add(1, Ordering::SeqCst) + 1;
        info!(count, "Processed message");
        
        // Simulate occasional failures
        if count.is_multiple_of(5) {
            anyhow::bail!("Simulated processing error at message {}", count);
        }
        
        Ok(())
    }

    async fn cleanup(&self) {
        info!("Cleaning up message processor");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting persistence example");
    info!("Task state will be persisted to ./task-state directory");

    let processed_count = Arc::new(AtomicUsize::new(0));

    // Create filesystem-based state store
    let state_dir = PathBuf::from("./task-state");
    
    // Clean up previous state for demo purposes
    if state_dir.exists() {
        std::fs::remove_dir_all(&state_dir).ok();
    }
    
    let store = FsStateStore::new(state_dir).await?;

    // Create supervisor with persistence
    let supervisor = Supervisor::new()
        .with_state_store(Arc::new(store))
        .add(MessageProcessor {
            processed_count: processed_count.clone(),
        });

    info!("Supervisor started with persistence enabled");
    info!("The task will persist its state (attempts, failures, last success)");
    info!("Press Ctrl+C to stop and observe state persistence");

    // Run until interrupted
    tokio::select! {
        result = supervisor.start_and_wait_any() => {
            if let Err(e) = result {
                info!(error = %e, "Supervisor encountered an error");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
    }

    let final_count = processed_count.load(Ordering::SeqCst);
    info!(final_count, "Example completed");
    info!("Check ./task-state directory to see persisted state");

    Ok(())
}
