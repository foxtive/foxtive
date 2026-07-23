//! Example demonstrating macro-based dependency injection with `#[derive(Service)]`.

use foxtive::lifecycle::Service;
use foxtive::prelude::AppResult;
use foxtive::App;
use std::sync::Arc;

#[derive(Service, Clone)]
struct CacheService {
    name: String,
}

impl Default for CacheService {
    fn default() -> Self {
        Self {
            name: "default-cache".to_string(),
        }
    }
}

#[derive(Service, Default)]
struct AuthService {
    #[dependency]
    cache: Arc<CacheService>,
}

#[derive(Service, Default)]
struct UserService {
    #[dependency]
    cache: Arc<CacheService>,
    #[dependency]
    auth: Arc<AuthService>,
}

#[tokio::main]
async fn main() -> AppResult<()> {
    println!("=== Macro-based DI Example ===\n");

    let app = App::builder("My App", "MYAPP")
        .register_service::<CacheService>()
        .register_service::<AuthService>()
        .register_service::<UserService>()
        .build()
        .await?;

    println!("=== All services initialized ===\n");

    let cache = app.require::<CacheService>()?;
    println!("CacheService: {}", cache.name);

    let auth = app.require::<AuthService>()?;
    println!("AuthService has cache: {}", auth.cache.name);

    let user = app.require::<UserService>()?;
    println!("UserService has cache: {}", user.cache.name);
    println!("UserService has auth with cache: {}", user.auth.cache.name);

    println!("\n=== Example complete ===");
    Ok(())
}
