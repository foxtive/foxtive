//! Example: Using foxtive with foxtive-supervisor for task orchestration.
//!
//! Run with:
//! ```shell
//! cargo run --example supervisor_integration
//! ```

use std::sync::Arc;
use std::time::Duration;

use foxtive::prelude::*;
use foxtive::Environment;

struct MetricsCollector;

impl MetricsCollector {
    async fn run(&self, cancellation: tokio_util::sync::CancellationToken) {
        println!("Metrics collector started");
        
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => {
                    println!("Metrics collector shutting down");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_secs(5)) => {
                    println!("Collecting metrics...");
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    let app: Arc<App> = App::builder("Supervisor Integration Demo", "DEMO")
        .environment(Environment::Local)
        .app_key("demo-secret-key")
        .register(MetricsCollector)
        .on_shutdown(|_app| async move {
            println!("Running shutdown hooks...");
        })
        .build()
        .await?;

    println!("App name: {}", app.app_name());
    println!("Environment: {:?}", app.env());

    let collector: Arc<MetricsCollector> = app.require()?;

    // In a real application you would use foxtive-supervisor:
    //
    // use foxtive_supervisor::Supervisor;
    //
    // let mut supervisor = Supervisor::new();
    //
    // supervisor.spawn("metrics-collector", |cancellation| {
    //     let collector = collector.clone();
    //     async move {
    //         collector.run(cancellation).await;
    //         Ok(())
    //     }
    // });
    //
    // // Wait for shutdown signal (Ctrl+C, SIGTERM, etc.)
    // tokio::signal::ctrl_c().await.ok();
    //
    // // Graceful shutdown
    // supervisor.shutdown().await;

    let cancellation = tokio_util::sync::CancellationToken::new();
    let cancel_clone = cancellation.clone();
    
    let handle = tokio::spawn(async move {
        collector.run(cancel_clone).await;
    });

    tokio::time::sleep(Duration::from_secs(2)).await;
    cancellation.cancel();

    handle.await.ok();

    app.shutdown().await;

    Ok(())
}
