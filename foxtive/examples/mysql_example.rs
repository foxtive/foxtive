//! MySQL backend example.
//!
//! This example demonstrates how to use foxtive with a MySQL database backend.
//! Requires MySQL 8.0+ for pagination window function support.
//!
//! # Running
//!
//! ```bash
//! cargo run --example mysql_example --features "database-mysql,tracing-setup"
//! ```
//!
//! # Environment
//!
//! Set the `MYSQL_DSN` environment variable:
//!
//! ```bash
//! export MYSQL_DSN="mysql://root:password@localhost:3306/mydb"
//! ```

use foxtive::database::DbConfig;
use foxtive::prelude::*;
use std::time::Duration;

#[tokio::main]
async fn main() -> AppResult<()> {
    // Read MySQL DSN from environment
    let dsn = std::env::var("MYSQL_DSN")
        .unwrap_or_else(|_| "mysql://root:password@localhost:3306/foxtive_test".to_string());

    // Configure the database connection pool
    let db_config = DbConfig::create(&dsn)
        .max_size(10)
        .min_idle(Some(2))
        .connection_timeout(Some(Duration::from_secs(5)))
        .max_lifetime(Some(Duration::from_secs(1800)))
        .idle_timeout(Some(Duration::from_secs(600)));

    // Build the app with MySQL backend
    let app = App::builder("mysql-example", "MXAMP")
        .environment(foxtive::Environment::Development)
        .app_key("example-secret-key")
        .database(db_config)
        .build()
        .await?;

    println!(
        "App: {} v{}",
        app.app_name(),
        app.version().unwrap_or("0.0.0")
    );
    println!("MySQL connection pool created successfully!");

    // Access the pool
    let pool = app.db()?;
    println!("Pool max_size: {}", pool.max_size());

    // Checkout a connection (blocking)
    // Note: In a real application, you would use diesel queries here
    // let mut conn = pool.connection()?;
    // let results = my_table::table.load::<MyModel>(&mut conn)?;

    println!("Example completed successfully!");
    Ok(())
}
