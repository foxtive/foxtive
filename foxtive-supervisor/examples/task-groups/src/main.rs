use anyhow::Result;
use async_trait::async_trait;
use foxtive_supervisor::{enums::HealthStatus, SupervisedTask, Supervisor};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{info, Level};

/// Database service - part of infrastructure group
struct DatabaseService {
    ready: Arc<AtomicBool>,
}

#[async_trait]
impl SupervisedTask for DatabaseService {
    fn id(&self) -> &'static str {
        "database"
    }

    fn group_id(&self) -> Option<&'static str> {
        Some("infrastructure")
    }

    async fn setup(&self) -> Result<()> {
        info!("Setting up database connection");
        sleep(Duration::from_millis(300)).await;
        self.ready.store(true, Ordering::SeqCst);
        info!("Database ready");
        Ok(())
    }

    async fn run(&self) -> Result<()> {
        // Simulate database health checks
        sleep(Duration::from_secs(2)).await;

        if !self.ready.load(Ordering::SeqCst) {
            anyhow::bail!("Database not ready");
        }

        info!("Database health check passed");
        Ok(())
    }

    async fn health_check(&self) -> HealthStatus {
        if self.ready.load(Ordering::SeqCst) {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy {
                reason: "Database not initialized".to_string(),
            }
        }
    }
}

/// Cache service - depends on database, part of infrastructure group
#[allow(unused)]
struct CacheService {
    db_ready: Arc<AtomicBool>,
    cache_ready: Arc<AtomicBool>,
}

#[async_trait]
impl SupervisedTask for CacheService {
    fn id(&self) -> &'static str {
        "cache"
    }

    fn group_id(&self) -> Option<&'static str> {
        Some("infrastructure")
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &["database"]
    }

    async fn setup(&self) -> Result<()> {
        info!("Setting up cache connection");
        sleep(Duration::from_millis(200)).await;
        self.cache_ready.store(true, Ordering::SeqCst);
        info!("Cache ready");
        Ok(())
    }

    async fn run(&self) -> Result<()> {
        // Simulate cache operations
        sleep(Duration::from_secs(2)).await;

        if !self.cache_ready.load(Ordering::SeqCst) {
            anyhow::bail!("Cache not ready");
        }

        info!("Cache health check passed");
        Ok(())
    }

    async fn health_check(&self) -> HealthStatus {
        if self.cache_ready.load(Ordering::SeqCst) {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded {
                reason: "Cache warming up".to_string(),
            }
        }
    }
}

/// API Server - depends on infrastructure, part of application group
#[allow(unused)]
struct ApiServer {
    db_ready: Arc<AtomicBool>,
    cache_ready: Arc<AtomicBool>,
}

#[async_trait]
impl SupervisedTask for ApiServer {
    fn id(&self) -> &'static str {
        "api-server"
    }

    fn group_id(&self) -> Option<&'static str> {
        Some("application")
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &["database", "cache"]
    }

    async fn setup(&self) -> Result<()> {
        info!("Setting up API server");
        sleep(Duration::from_millis(400)).await;
        info!("API server ready on port 8080");
        Ok(())
    }

    async fn run(&self) -> Result<()> {
        // Simulate handling requests
        sleep(Duration::from_secs(2)).await;
        info!("API server processing requests");
        Ok(())
    }

    async fn health_check(&self) -> HealthStatus {
        HealthStatus::Healthy
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting task groups example");
    info!("Demonstrates atomic group operations and health monitoring");

    let db_ready = Arc::new(AtomicBool::new(false));
    let cache_ready = Arc::new(AtomicBool::new(false));

    let supervisor = Supervisor::new()
        .add(DatabaseService {
            ready: db_ready.clone(),
        })
        .add(CacheService {
            db_ready: db_ready.clone(),
            cache_ready: cache_ready.clone(),
        })
        .add(ApiServer {
            db_ready: db_ready.clone(),
            cache_ready: cache_ready.clone(),
        });

    let mut runtime = supervisor.start().await?;

    info!("\n=== Starting Infrastructure Group ===");
    let started = runtime.start_group("infrastructure");
    info!(started, "Infrastructure tasks started");

    // Wait for infrastructure to initialize
    sleep(Duration::from_secs(2)).await;

    // Check infrastructure health
    let infra_health = runtime.get_group_health("infrastructure").await;
    match &infra_health {
        HealthStatus::Healthy => info!("Infrastructure status: HEALTHY"),
        HealthStatus::Degraded { reason } => {
            info!(reason, "Infrastructure status: DEGRADED")
        }
        HealthStatus::Unhealthy { reason } => {
            info!(reason, "Infrastructure status: UNHEALTHY")
        }
        _ => info!("Infrastructure status: UNKNOWN"),
    }

    // Get detailed health information
    let details = runtime.get_group_health_details("infrastructure").await;
    info!("\nInfrastructure group details:");
    for task in details {
        info!("  - {}: {:?}", task.id, task.health);
    }

    info!("\n=== Starting Application Group ===");
    let started = runtime.start_group("application");
    info!(started, "Application tasks started");

    // Monitor for a bit
    sleep(Duration::from_secs(3)).await;

    // Check all group health
    let app_health = runtime.get_group_health("application").await;
    info!("\nApplication group status: {:?}", app_health);

    info!("\n=== Restarting Infrastructure Group ===");
    let restarted = runtime.restart_group("infrastructure");
    info!(restarted, "Infrastructure tasks restarted");

    // Let it run briefly
    sleep(Duration::from_secs(2)).await;

    info!("\n=== Stopping Application Group ===");
    let stopped = runtime.stop_group("application").await;
    info!(stopped, "Application tasks stopped");

    // Clean shutdown
    runtime.shutdown().await;

    info!("\nExample completed successfully!");
    info!("Key takeaways:");
    info!("  - Tasks can be grouped for atomic operations");
    info!("  - Group health aggregates individual task health");
    info!("  - Groups enable organized task management");

    Ok(())
}
