//! Example demonstrating deferred trait binding with `register_trait_with`.
//!
//! Shows how to register trait objects that depend on services constructed
//! during `freeze()`.

use foxtive::lifecycle::ServiceInit;
use foxtive::prelude::*;
use std::sync::Arc;

// Define a trait
trait Notifier: Send + Sync {
    fn notify(&self, msg: &str) -> String;
}

// Service that will implement the trait
struct EmailNotifier {
    config: Arc<ConfigService>,
}

impl Notifier for EmailNotifier {
    fn notify(&self, msg: &str) -> String {
        format!("[EMAIL via {}] {}", self.config.smtp_host, msg)
    }
}

struct ConfigService {
    smtp_host: String,
}

impl ServiceInit for ConfigService {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self {
            smtp_host: "smtp.example.com".to_string(),
        })
    }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    println!("=== Deferred Trait Binding Example ===\n");

    // Register services and trait factory
    let app = App::builder("Deferred Trait Demo", "DTD")
        .register_service::<ConfigService>()
        // Defer trait binding construction until freeze()
        .register_trait_with::<dyn Notifier, _>(|app| {
            // Factory runs during freeze() - services are available
            let config = app.require::<ConfigService>()?;
            let notifier = Arc::new(EmailNotifier { config });
            Ok(notifier as Arc<dyn Notifier>)
        })
        .build()
        .await?;

    // Use the trait object
    let notifier = app.require_trait::<dyn Notifier>()?;
    println!("{}", notifier.notify("Hello from deferred trait binding!"));

    println!("\n=== Example complete ===");
    Ok(())
}
