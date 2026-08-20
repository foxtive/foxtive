//! Example: Using foxtive with an Axum web server.
//!
//! Run with:
//! ```shell
//! cargo run --example axum_integration
//! ```

use std::sync::Arc;

use foxtive::Environment;
use foxtive::prelude::*;

struct GreetingService;

impl GreetingService {
    fn greet(&self, name: &str) -> String {
        format!("Hello, {name}! Welcome to Foxtive.")
    }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    let app = App::builder("Axum Integration Demo", "DEMO")
        .environment(Environment::Local)
        .app_key("demo-secret-key")
        .register(GreetingService)
        .build()
        .await?;

    println!("App name: {}", app.app_name());
    println!("Environment: {:?}", app.env());

    let svc: Arc<GreetingService> = app.require()?;
    println!("{}", svc.greet("Developer"));

    // In a real application you would wire Axum routes here:
    //
    // use axum::{Router, routing::get, extract::State};
    //
    // let router = Router::new()
    //     .route("/health", get(|| async { "OK" }))
    //     .with_state(app);
    //
    // let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    // axum::serve(listener, router).await?;

    Ok(())
}
