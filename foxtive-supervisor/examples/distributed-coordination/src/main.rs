//! Example: Multi-Instance Coordination with Leader Election
//!
//! Run multiple supervisor instances that coordinate via Redis to ensure only one
//! instance (the leader) executes certain tasks.
//!
//! Run with: cargo run --example distributed-coordination --features distributed

use foxtive_supervisor::distributed::{CoordinationConfig, RedisCoordination};
use foxtive_supervisor::{SupervisedTask, Supervisor};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

/// A task that should only run on the leader instance
struct LeaderOnlyTask {
    name: &'static str,
}

#[async_trait::async_trait]
impl SupervisedTask for LeaderOnlyTask {
    fn id(&self) -> &'static str {
        self.name
    }

    async fn run(&self) -> anyhow::Result<()> {
        info!("Leader-only task '{}' is running", self.name);
        
        // Simulate work
        tokio::time::sleep(Duration::from_secs(5)).await;
        
        info!("Leader task '{}' completed", self.name);
        Ok(())
    }
}

/// A task that runs on all instances
struct BackgroundWorker {
    id: usize,
}

#[async_trait::async_trait]
impl SupervisedTask for BackgroundWorker {
    fn id(&self) -> &'static str {
        "background-worker"
    }

    async fn run(&self) -> anyhow::Result<()> {
        info!("Background worker {} running on this instance", self.id);
        
        loop {
            // Do some background work
            tokio::time::sleep(Duration::from_secs(2)).await;
            info!("Background worker {} heartbeat", self.id);
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let instance_id = format!("instance-{}", std::process::id());
    info!("Starting supervisor instance: {}", instance_id);

    // Create coordination backend
    let config = CoordinationConfig::default()
        .with_instance_id(&instance_id)
        .with_redis_url("redis://localhost:6379")
        .with_leader_lease(30)
        .with_heartbeat_interval(5);

    let coordination = Arc::new(RedisCoordination::new(config.clone()).await?);

    // Start coordination manager (handles leader election and heartbeats)
    let manager = foxtive_supervisor::distributed::redis_impl::CoordinationManager::new(
        coordination.clone(),
        config.clone(),
    );
    manager.start().await?;

    info!("Coordination started. Waiting to see if we become leader...");

    // Wait a bit for leader election
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Build supervisor based on leadership status
    let mut supervisor = Supervisor::new();

    // Add background workers that run on ALL instances
    supervisor = supervisor.add(BackgroundWorker { id: 1 });
    supervisor = supervisor.add(BackgroundWorker { id: 2 });

    // Check if we're the leader
    if manager.is_leader() {
        info!("✓ We are the LEADER - adding leader-only tasks");
        
        // Add tasks that should only run on the leader
        supervisor = supervisor.add(LeaderOnlyTask {
            name: "leader-report-generator",
        });
        supervisor = supervisor.add(LeaderOnlyTask {
            name: "leader-cleanup-job",
        });
    } else {
        info!("✗ We are a FOLLOWER - running only background tasks");
        
        if let Ok(Some(leader)) = coordination.get_current_leader().await {
            info!("Current leader is: {}", leader);
        }
    }

    // Start the supervisor
    info!("Starting supervisor with tasks...");
    let runtime = supervisor.start().await?;

    // Let it run for a while
    info!("Supervisor running. Press Ctrl+C to stop.");
    
    // Monitor leadership changes
    let coord_clone = coordination.clone();
    let instance_clone = instance_id.clone();
    
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            
            match coord_clone.is_leader(&instance_clone).await {
                Ok(true) => info!("Still the leader"),
                Ok(false) => warn!("No longer the leader!"),
                Err(e) => warn!("Failed to check leadership: {}", e),
            }
        }
    });

    // Wait indefinitely (or until shutdown signal)
    tokio::signal::ctrl_c().await?;
    info!("Shutdown signal received");

    // Shutdown gracefully
    runtime.shutdown().await;
    info!("Supervisor shut down successfully");

    Ok(())
}
