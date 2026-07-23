//! Example demonstrating the ServiceInit-based DI system.

use foxtive::lifecycle::ServiceInit;
use foxtive::prelude::*;

struct CacheService {
    name: String,
}

impl ServiceInit for CacheService {
    async fn init(app: &App) -> AppResult<Self> {
        println!("Initializing CacheService for app: {}", app.app_name());
        Ok(Self {
            name: "cache".to_string(),
        })
    }
}

struct UserService {
    cache_name: String,
}

impl ServiceInit for UserService {
    async fn init(app: &App) -> AppResult<Self> {
        println!("Initializing UserService");
        let cache = app.require::<CacheService>()?;
        Ok(Self {
            cache_name: cache.name.clone(),
        })
    }
}

struct AuthService {
    app_name: String,
    #[allow(dead_code)]
    token_secret: String,
}

impl ServiceInit for AuthService {
    async fn init(app: &App) -> AppResult<Self> {
        println!("Initializing AuthService");
        Ok(Self {
            app_name: app.app_name().to_string(),
            token_secret: "secret".to_string(),
        })
    }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    tracing_subscriber::fmt::init();

    println!("=== ServiceInit DI Example ===\n");

    let app = App::builder("My Service", "MYSVC")
        .register_service::<CacheService>()
        .register_service::<AuthService>()
        .register_service::<UserService>()
        .build()
        .await?;

    println!("\n=== All services initialized ===\n");

    let cache = app.get::<CacheService>().unwrap();
    println!("CacheService: {}", cache.name);

    let auth_service = app.get::<AuthService>().unwrap();
    println!("AuthService app_name: {}", auth_service.app_name);

    let user_service = app.get::<UserService>().unwrap();
    println!("UserService has cache: {}", user_service.cache_name);

    println!("\n=== Example complete ===");

    Ok(())
}
