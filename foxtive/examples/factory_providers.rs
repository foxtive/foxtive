//! Example demonstrating factory/closure providers via `register_with`.
//!
//! Shows how to register types that don't implement `ServiceInit`
//! using a factory closure.

use foxtive::lifecycle::ServiceInit;
use foxtive::prelude::*;

// A "foreign" type that doesn't implement ServiceInit
struct HttpClient {
    base_url: String,
    timeout_ms: u64,
}

// A service that depends on the foreign type
struct ApiService {
    #[allow(dead_code)]
    client_url: String,
}

impl ServiceInit for ApiService {
    async fn init(app: &App) -> AppResult<Self> {
        let client = app.require::<HttpClient>()?;
        Ok(Self {
            client_url: client.base_url.clone(),
        })
    }
}

// A service that uses a factory to extract deps synchronously
struct ConfiguredService {
    #[allow(dead_code)]
    config_value: String,
}

#[tokio::main]
async fn main() -> AppResult<()> {
    tracing_subscriber::fmt::init();

    println!("=== Factory Providers Example ===\n");

    // Register a foreign type via closure
    let app = App::builder("Factory Demo", "FCTDM")
        .register_with(|_app| async {
            Ok(HttpClient {
                base_url: "https://api.example.com".into(),
                timeout_ms: 5000,
            })
        })
        .register_service::<ApiService>()
        .register_with(|app| {
            // Extract dependencies synchronously before the async block
            let app_name = app.app_name().to_string();
            async move {
                Ok(ConfiguredService {
                    config_value: format!("config-for-{app_name}"),
                })
            }
        })
        .build()
        .await?;

    let client = app.get::<HttpClient>().unwrap();
    println!("HttpClient base_url: {}", client.base_url);
    println!("HttpClient timeout: {}ms", client.timeout_ms);

    let api = app.get::<ApiService>().unwrap();
    println!("ApiService client_url: {}", api.client_url);

    let configured = app.get::<ConfiguredService>().unwrap();
    println!("ConfiguredService config: {}", configured.config_value);

    println!("\n=== Example complete ===");
    Ok(())
}
