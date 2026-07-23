//! # Database + Message Consumer Example
//!
//! This example shows a common pattern:
//! 1. A Database Pool that must be ready first.
//! 2. Multiple consumers that process different message queues, all dependent on the DB.
//! 3. Use of persistence to track consumer progress (simulated by failure counts).

use foxtive_supervisor::persistence::FsStateStore;
use foxtive_supervisor::{SupervisedTask, Supervisor};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

struct DbPool;

#[async_trait::async_trait]
impl SupervisedTask for DbPool {
    fn id(&self) -> &'static str {
        "db-pool"
    }
    async fn setup(&self) -> foxtive_supervisor::SupervisorResult<()> {
        info!("Connecting to the database...");
        tokio::time::sleep(Duration::from_secs(1)).await;
        Ok(())
    }
    async fn run(&self) -> foxtive_supervisor::SupervisorResult<()> {
        info!("Database pool ready.");
        tokio::time::sleep(Duration::MAX).await;
        Ok(())
    }
}

struct EmailConsumer;

#[async_trait::async_trait]
impl SupervisedTask for EmailConsumer {
    fn id(&self) -> &'static str {
        "email-consumer"
    }
    fn dependencies(&self) -> &'static [&'static str] {
        &["db-pool"]
    }

    async fn run(&self) -> foxtive_supervisor::SupervisorResult<()> {
        info!("Email consumer starting...");
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            info!("Sent email notification batch.");
        }
    }
}

struct BillingConsumer;

#[async_trait::async_trait]
impl SupervisedTask for BillingConsumer {
    fn id(&self) -> &'static str {
        "billing-consumer"
    }
    fn dependencies(&self) -> &'static [&'static str] {
        &["db-pool"]
    }

    async fn run(&self) -> foxtive_supervisor::SupervisorResult<()> {
        info!("Billing consumer starting...");
        // Simulate a flakey connection that gets better over time (persisted state)
        tokio::time::sleep(Duration::from_secs(2)).await;
        return Err(foxtive_supervisor::SupervisorError::from("Billing API timeout (simulated)"));
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // Create a persistence store
    let store_path = "./.supervisor-state";
    let store = Arc::new(FsStateStore::new(store_path).await?);
    info!("Using persistence store at: {}", store_path);

    let supervisor = Supervisor::new()
        .with_state_store(store)
        .add(DbPool)
        .add(EmailConsumer)
        .add(BillingConsumer);

    info!("Starting supervisor...");
    let runtime = supervisor.start().await?;

    // Wait for a while to see the billing consumer retry and persist its state
    tokio::time::sleep(Duration::from_secs(10)).await;

    info!("Retrieving billing consumer info...");
    let info = runtime.get_task_info("billing-consumer").await?;
    info!("Task Info: ID={}, Health={:?}", info.id, info.health);

    info!("Shutting down...");
    runtime.shutdown().await;

    Ok(())
}
