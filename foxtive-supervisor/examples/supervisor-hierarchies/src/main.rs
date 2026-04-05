use anyhow::Result;
use async_trait::async_trait;
use foxtive_supervisor::hierarchy::SupervisorHierarchy;
use foxtive_supervisor::{SupervisedTask, Supervisor};
use tokio::time::{sleep, Duration};
use tracing::{info, Level};
use tracing_subscriber;

/// Authentication service
struct AuthService;

#[async_trait]
impl SupervisedTask for AuthService {
    fn id(&self) -> &'static str {
        "auth-service"
    }

    async fn setup(&self) -> Result<()> {
        info!("Setting up authentication service");
        sleep(Duration::from_millis(200)).await;
        Ok(())
    }

    async fn run(&self) -> Result<()> {
        sleep(Duration::from_secs(2)).await;
        info!("Auth service processing requests");
        Ok(())
    }
}

/// User management service
struct UserService;

#[async_trait]
impl SupervisedTask for UserService {
    fn id(&self) -> &'static str {
        "user-service"
    }

    async fn setup(&self) -> Result<()> {
        info!("Setting up user service");
        sleep(Duration::from_millis(300)).await;
        Ok(())
    }

    async fn run(&self) -> Result<()> {
        sleep(Duration::from_secs(2)).await;
        info!("User service handling operations");
        Ok(())
    }
}

/// Email notification worker
struct EmailWorker;

#[async_trait]
impl SupervisedTask for EmailWorker {
    fn id(&self) -> &'static str {
        "email-worker"
    }

    async fn setup(&self) -> Result<()> {
        info!("Setting up email worker");
        sleep(Duration::from_millis(150)).await;
        Ok(())
    }

    async fn run(&self) -> Result<()> {
        sleep(Duration::from_secs(3)).await;
        info!("Email worker sending notifications");
        Ok(())
    }
}

/// Report generation worker
struct ReportWorker;

#[async_trait]
impl SupervisedTask for ReportWorker {
    fn id(&self) -> &'static str {
        "report-worker"
    }

    async fn setup(&self) -> Result<()> {
        info!("Setting up report worker");
        sleep(Duration::from_millis(250)).await;
        Ok(())
    }

    async fn run(&self) -> Result<()> {
        sleep(Duration::from_secs(4)).await;
        info!("Report worker generating reports");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting supervisor hierarchies example");
    info!("This demonstrates nested supervisor trees with cascading shutdown");

    // Create API services supervisor
    let api_supervisor = Supervisor::new()
        .add(AuthService)
        .add(UserService);

    // Create background workers supervisor
    let worker_supervisor = Supervisor::new()
        .add(EmailWorker)
        .add(ReportWorker);

    info!("\n=== Building Hierarchy ===");

    // Build hierarchy
    let hierarchy = SupervisorHierarchy::new("root")
        .add_child("api-services", api_supervisor)
        .add_child("background-workers", worker_supervisor);

    info!("\nStarting all supervisors in hierarchy...");
    
    // Start the entire hierarchy (bottom-up)
    let runtime = hierarchy.start_all().await?;
    
    info!("Hierarchy started with {} total tasks", runtime.total_task_count());

    // Let them run briefly
    info!("\nRunning for 5 seconds...");
    sleep(Duration::from_secs(5)).await;

    info!("\n=== Initiating Cascading Shutdown ===");
    info!("Shutdown will propagate from root to all children");

    // Shutdown the entire hierarchy (top-down, children in parallel)
    runtime.shutdown_all().await;

    info!("\nAll supervisors shut down successfully!");
    info!("\nKey concepts demonstrated:");
    info!("  - Nested supervisor hierarchies for organized architecture");
    info!("  - Parent-child relationships between supervisors");
    info!("  - Cascading shutdown propagation");
    info!("  - Organized task grouping by domain");

    Ok(())
}
