//! Example demonstrating mixed sync and async service registration.

use foxtive::lifecycle::ServiceInit;
use foxtive::prelude::*;

struct ConfigService {
    database_url: String,
}

struct DatabaseService {
    pool_size: usize,
}

impl ServiceInit for DatabaseService {
    async fn init(app: &App) -> AppResult<Self> {
        println!("Initializing DatabaseService...");
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let config = app.require::<ConfigService>()?;
        println!("Database connected to: {}", config.database_url);

        Ok(Self { pool_size: 10 })
    }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    tracing_subscriber::fmt::init();

    println!("=== Mixed Registration Example ===\n");

    let mut init = App::builder("Mixed App", "MIXED")
        .register(ConfigService {
            database_url: "postgres://localhost/mydb".to_string(),
        })
        .register_service::<DatabaseService>()
        .build_init()
        .await?;

    init.register(42i32);

    let app = init.freeze().await?;

    println!("\n=== All services ready ===\n");

    let config = app.get::<ConfigService>().unwrap();
    println!("Config - DB: {}", config.database_url);

    let db = app.get::<DatabaseService>().unwrap();
    println!("Database pool size: {}", db.pool_size);

    let number = app.get::<i32>().unwrap();
    println!("Registered number: {}", number);

    println!("\n=== Example complete ===");

    Ok(())
}
