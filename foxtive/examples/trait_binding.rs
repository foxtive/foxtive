//! Example demonstrating trait binding in the DI container.
//!
//! Shows how to register trait objects (`Arc<dyn Trait>`) and resolve
//! them via `app.require_trait::<dyn Trait>()`.

use foxtive::prelude::*;
use std::sync::Arc;

// Define a trait
trait Notifier: Send + Sync {
    fn notify(&self, msg: &str) -> String;
}

// Two implementations
struct EmailNotifier;
impl Notifier for EmailNotifier {
    fn notify(&self, msg: &str) -> String {
        format!("[EMAIL] {msg}")
    }
}

struct SmsNotifier;
impl Notifier for SmsNotifier {
    fn notify(&self, msg: &str) -> String {
        format!("[SMS] {msg}")
    }
}

// A second trait to show multiple bindings
trait Logger: Send + Sync {
    fn log(&self, msg: &str) -> String;
}

struct ConsoleLogger;
impl Logger for ConsoleLogger {
    fn log(&self, msg: &str) -> String {
        format!("[LOG] {msg}")
    }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    tracing_subscriber::fmt::init();

    println!("=== Trait Binding Example ===\n");

    let app = App::builder("Trait Demo", "TRTDM")
        .register_trait::<dyn Notifier>(Arc::new(EmailNotifier))
        .register_trait::<dyn Logger>(Arc::new(ConsoleLogger))
        .build()
        .await?;

    // Resolve trait bindings
    let notifier = app.require_trait::<dyn Notifier>()?;
    println!("{}", notifier.notify("Hello from trait binding!"));

    let logger = app.require_trait::<dyn Logger>()?;
    println!("{}", logger.log("Logging via trait object"));

    // get_trait returns Option (no error if missing)
    assert!(app.get_trait::<dyn Notifier>().is_some());
    assert!(app.get_trait::<dyn Logger>().is_some());

    // Replace a trait binding
    let mut init = App::builder("Trait Demo 2", "TRTD2")
        .register_trait::<dyn Notifier>(Arc::new(EmailNotifier))
        .build_init()
        .await?;

    init.register_trait::<dyn Notifier>(Arc::new(SmsNotifier));
    let app2 = init.freeze().await?;

    let notifier2 = app2.require_trait::<dyn Notifier>()?;
    println!("{}", notifier2.notify("Replaced with SMS notifier"));

    println!("\n=== Example complete ===");
    Ok(())
}
