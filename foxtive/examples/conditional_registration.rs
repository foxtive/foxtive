//! Example demonstrating conditional and idempotent registration.
//!
//! Shows `register_service_if`, `try_register_service`, `replace_service`,
//! and `register_if`.

use foxtive::lifecycle::ServiceInit;
use foxtive::prelude::*;

struct MetricsService {
    #[allow(dead_code)]
    enabled: bool,
}

impl ServiceInit for MetricsService {
    async fn init(_app: &App) -> AppResult<Self> {
        println!("MetricsService constructed");
        Ok(Self { enabled: true })
    }
}

struct LoggingService {
    #[allow(dead_code)]
    level: String,
}

impl ServiceInit for LoggingService {
    async fn init(_app: &App) -> AppResult<Self> {
        println!("LoggingService constructed");
        Ok(Self {
            level: "info".to_string(),
        })
    }
}

struct OverrideService {
    #[allow(dead_code)]
    version: u32,
}

impl ServiceInit for OverrideService {
    async fn init(_app: &App) -> AppResult<Self> {
        println!("OverrideService constructed");
        Ok(Self { version: 1 })
    }
}

struct OverrideServiceV2 {
    #[allow(dead_code)]
    version: u32,
}

impl ServiceInit for OverrideServiceV2 {
    async fn init(_app: &App) -> AppResult<Self> {
        println!("OverrideServiceV2 constructed (replacement)");
        Ok(Self { version: 2 })
    }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    tracing_subscriber::fmt::init();

    println!("=== Conditional Registration Example ===\n");

    // 1. register_service_if: conditionally register
    let enable_metrics = true;
    let enable_logging = false;

    let app = App::builder("Conditional Demo", "CNDM")
        .register_service_if::<MetricsService>(enable_metrics)
        .register_service_if::<LoggingService>(enable_logging)
        // 2. register_if: conditionally register an instance
        .register_if(true, 42u32)
        .register_if(false, "should-not-appear".to_string())
        .build()
        .await?;

    assert!(app.get::<MetricsService>().is_some(), "MetricsService should be registered");
    assert!(app.get::<LoggingService>().is_none(), "LoggingService should NOT be registered");
    assert!(app.get::<u32>().is_some(), "u32 should be registered");
    assert!(app.get::<String>().is_none(), "String should NOT be registered");
    println!("Conditional registration works correctly!");

    // 3. try_register_service: idempotent (silent skip on duplicate)
    let app2 = App::builder("Idempotent Demo", "IDPDM")
        .register_service::<MetricsService>()
        .try_register_service::<MetricsService>() // silently skipped
        .build()
        .await?;

    assert!(app2.get::<MetricsService>().is_some());
    println!("Idempotent registration works correctly!");

    // 4. replace_service: explicit override
    let app3 = App::builder("Replace Demo", "RPLDM")
        .register_service::<OverrideService>()
        .replace_service::<OverrideServiceV2>() // replaces with V2
        .build()
        .await?;

    assert!(app3.get::<OverrideServiceV2>().is_some(), "V2 should be registered after replace");
    println!("Service replacement works correctly!");

    println!("\n=== Example complete ===");
    Ok(())
}
