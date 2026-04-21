//! # Microservice Orchestration Example
//!
//! Orchestrate interdependent services with:
//! - A Database Pool (Dependency)
//! - A Cache Service (Dependency)
//! - An API Gateway (Depends on DB and Cache)
//! - A Message Consumer (Depends on DB)
//! - Dynamic Service Management (Adding a task at runtime)
//! - Concurrency and Priority Control

use foxtive_supervisor::contracts::SupervisorEventListener;
use foxtive_supervisor::enums::{BackoffStrategy, HealthStatus, RestartPolicy, SupervisorEvent};
use foxtive_supervisor::{SupervisedTask, Supervisor};
use std::time::Duration;
use tokio::signal;
use tracing::{error, info, warn};

// --- 1. Database Service ---

struct DatabasePool;

#[async_trait::async_trait]
impl SupervisedTask for DatabasePool {
    fn id(&self) -> &'static str {
        "database"
    }
    fn priority(&self) -> i32 {
        100
    } // Start first

    async fn setup(&self) -> anyhow::Result<()> {
        info!("[Database] Establishing connection pool...");
        tokio::time::sleep(Duration::from_millis(500)).await;
        Ok(())
    }

    async fn run(&self) -> anyhow::Result<()> {
        info!("[Database] Connection pool is active.");
        // Keep-alive loop
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            info!("[Database] Health check: OK");
        }
    }

    async fn health_check(&self) -> HealthStatus {
        HealthStatus::Healthy
    }
}

// --- 2. Cache Service ---

struct CacheService;

#[async_trait::async_trait]
impl SupervisedTask for CacheService {
    fn id(&self) -> &'static str {
        "cache"
    }
    fn priority(&self) -> i32 {
        90
    }

    async fn setup(&self) -> anyhow::Result<()> {
        info!("[Cache] Connecting to Redis...");
        tokio::time::sleep(Duration::from_millis(300)).await;
        Ok(())
    }

    async fn run(&self) -> anyhow::Result<()> {
        info!("[Cache] Connected and listening.");
        tokio::time::sleep(Duration::MAX).await;
        Ok(())
    }
}

// --- 3. API Gateway ---

struct ApiGateway;

#[async_trait::async_trait]
impl SupervisedTask for ApiGateway {
    fn id(&self) -> &'static str {
        "api-gateway"
    }
    fn dependencies(&self) -> &'static [&'static str] {
        &["database", "cache"]
    }

    async fn run(&self) -> anyhow::Result<()> {
        info!("[API Gateway] Listening on :8080 (DB and Cache ready)");

        // Simulate a crash after some time
        tokio::time::sleep(Duration::from_secs(15)).await;
        warn!("[API Gateway] Encountered a memory leak, crashing...");
        anyhow::bail!("Out of memory");
    }

    fn restart_policy(&self) -> RestartPolicy {
        RestartPolicy::Always
    }

    fn backoff_strategy(&self) -> BackoffStrategy {
        BackoffStrategy::exponential_custom(Duration::from_secs(1), Duration::from_secs(10))
    }
}

// --- 4. Order Consumer ---

struct OrderConsumer;

#[async_trait::async_trait]
impl SupervisedTask for OrderConsumer {
    fn id(&self) -> &'static str {
        "order-consumer"
    }
    fn dependencies(&self) -> &'static [&'static str] {
        &["database"]
    }

    async fn run(&self) -> anyhow::Result<()> {
        info!("[Order Consumer] Starting message processing...");
        let mut count = 0;
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            count += 1;
            info!("[Order Consumer] Processed order #{}", count);
        }
    }
}

// --- 5. Event Listener ---

#[allow(unused)]
struct MetricsListener;

#[async_trait::async_trait]
impl SupervisorEventListener for MetricsListener {
    async fn on_event(&self, event: SupervisorEvent) {
        match event {
            SupervisorEvent::TaskFailed {
                id, error, attempt, ..
            } => {
                error!(
                    "CRITICAL: Task {} failed on attempt {} with error: {}",
                    id, attempt, error
                );
            }
            SupervisorEvent::TaskStarted { id, attempt, .. } if attempt > 1 => {
                info!("Task {} restarted (attempt {})", id, attempt);
            }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    info!("Initializing Microservice Orchestrator...");

    // Configure the supervisor
    let supervisor = Supervisor::new()
        .with_global_concurrency_limit(10)
        .add(DatabasePool)
        .add(CacheService)
        .add(ApiGateway)
        .add(OrderConsumer);

    // Start the supervisor
    let mut runtime = supervisor.start().await?;

    info!("System is running. Managing dynamic services...");

    // Simulate adding a dynamic task after some time
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        info!("--- Adding Analytics Service at runtime ---");

        struct AnalyticsService;
        #[async_trait::async_trait]
        impl SupervisedTask for AnalyticsService {
            fn id(&self) -> &'static str {
                "analytics"
            }
            async fn run(&self) -> anyhow::Result<()> {
                info!("[Analytics] Aggregating data...");
                loop {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    info!("[Analytics] Heartbeat: Processing aggregation");
                }
            }
        }

        if let Err(e) = runtime.add_task(AnalyticsService) {
            error!("Failed to add dynamic task: {}", e);
        }
    });

    // Wait for shutdown signal
    signal::ctrl_c().await?;
    info!("Shutdown signal received. Gracefully stopping services...");

    Ok(())
}
