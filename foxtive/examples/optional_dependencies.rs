//! Example demonstrating optional dependencies with `Option<Arc<T>>` and `Option<T>`.
//!
//! Shows how services can declare optional dependencies that resolve to
//! `None` when the dependency is not registered.

use foxtive::lifecycle::Service;
use foxtive::prelude::*;
use std::sync::Arc;

// An optional cache service
struct CacheService {
    #[allow(dead_code)]
    driver: String,
}

// A required config service
struct ConfigService {
    #[allow(dead_code)]
    app_name: String,
}

// A service with both required and optional dependencies
struct BusinessService {
    #[allow(dead_code)]
    config: Arc<ConfigService>,
    #[allow(dead_code)]
    cache: Option<Arc<CacheService>>,
    #[allow(dead_code)]
    timeout: Option<u32>,
}

// Manual ServiceInit to show optional deps without the macro
impl foxtive::lifecycle::ServiceInit for BusinessService {
    async fn init(app: &App) -> AppResult<Self> {
        Ok(Self {
            config: app.require::<ConfigService>()?,
            cache: app.get::<CacheService>(),       // Option<Arc<CacheService>>
            timeout: app.get::<u32>().map(|v| *v),  // Option<u32>
        })
    }
}

// A service using the derive macro with optional deps
#[derive(Service)]
struct DerivedService {
    #[dependency]
    config: Arc<ConfigService>,
    #[dependency]
    cache: Option<Arc<CacheService>>,
}

#[tokio::main]
async fn main() -> AppResult<()> {
    tracing_subscriber::fmt::init();

    println!("=== Optional Dependencies Example ===\n");

    // Case 1: Optional deps are present
    println!("--- Case 1: All deps present ---");
    let app = App::builder("Optional Demo", "OPTDM")
        .register(ConfigService {
            app_name: "demo".into(),
        })
        .register(CacheService {
            driver: "redis".into(),
        })
        .register(42u32)
        .register_service::<BusinessService>()
        .register_service::<DerivedService>()
        .build()
        .await?;

    let biz = app.get::<BusinessService>().unwrap();
    println!("config.app_name: {}", biz.config.app_name);
    println!(
        "cache: {}",
        biz.cache.as_ref().map(|c| c.driver.as_str()).unwrap_or("None")
    );
    println!("timeout: {:?}", biz.timeout);

    let derived = app.get::<DerivedService>().unwrap();
    println!("derived.config: {}", derived.config.app_name);
    println!(
        "derived.cache: {}",
        derived.cache.as_ref().map(|c| c.driver.as_str()).unwrap_or("None")
    );

    // Case 2: Optional deps are absent
    println!("\n--- Case 2: Optional deps absent ---");
    let app2 = App::builder("Optional Demo 2", "OPTD2")
        .register(ConfigService {
            app_name: "minimal".into(),
        })
        // CacheService and u32 are NOT registered
        .register_service::<BusinessService>()
        .register_service::<DerivedService>()
        .build()
        .await?;

    let biz2 = app2.get::<BusinessService>().unwrap();
    println!("config.app_name: {}", biz2.config.app_name);
    println!(
        "cache: {}",
        biz2.cache.as_ref().map(|c| c.driver.as_str()).unwrap_or("None")
    );
    println!("timeout: {:?}", biz2.timeout);

    println!("\n=== Example complete ===");
    Ok(())
}
