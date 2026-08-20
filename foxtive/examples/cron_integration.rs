//! Example: Using foxtive with foxtive-cron for scheduled job execution.
//!
//! Run with:
//! ```shell
//! cargo run --example cron_integration
//! ```

use std::sync::Arc;
use std::time::Duration;

use foxtive::Environment;
use foxtive::prelude::*;

struct DataCleanup;

impl DataCleanup {
    async fn run(&self) -> AppResult<()> {
        println!("Running data cleanup task...");
        tokio::time::sleep(Duration::from_millis(100)).await;
        println!("Cleanup complete");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    let app = App::builder("Cron Integration Demo", "DEMO")
        .environment(Environment::Local)
        .app_key("demo-secret-key")
        .register(DataCleanup)
        .build()
        .await?;

    println!("App name: {}", app.app_name());
    println!("Environment: {:?}", app.env());

    let cleanup: Arc<DataCleanup> = app.require()?;

    // In a real application you would use foxtive-cron:
    //
    // use foxtive_cron::{Cron, Job};
    //
    // let mut cron = Cron::new();
    //
    // // Run every day at midnight
    // cron.add_job(
    //     Job::new("data-cleanup", "0 0 * * * *", move || {
    //         let cleanup = cleanup.clone();
    //         async move {
    //             cleanup.run().await
    //         }
    //     })
    //     .with_timezone("America/New_York")
    // );
    //
    // cron.start().await?;

    cleanup.run().await?;

    Ok(())
}
