//! Demonstrates three ways to register mutable services in foxtive.

use foxtive::lifecycle::Service;
use foxtive::prelude::*;
use std::sync::Arc;

struct ManualCounter {
    count: u64,
}

impl ManualCounter {
    fn new() -> Self {
        Self { count: 0 }
    }

    fn increment(&mut self) {
        self.count += 1;
    }

    fn value(&self) -> u64 {
        self.count
    }
}

#[derive(Service)]
struct DeferredState {
    #[dependency]
    counter: Arc<Mutable<ManualCounter>>,
    label: String,
}

#[derive(Service)]
#[service(mutable)]
struct AppState {
    requests: u64,
    errors: u64,
}

#[tokio::main]
async fn main() -> AppResult<()> {
    println!("=== Mutable Service Registration ===\n");

    let app = App::builder("mutable-demo", "DEMO")
        .after_build(|init: &mut AppInit| {
            init.register_mutable(ManualCounter::new());
            Ok(())
        })
        .register_mutable_service::<DeferredState>()
        .register_service::<AppState>()
        .build()
        .await?;

    let counter = app.require_mutable::<ManualCounter>()?;
    println!("ManualCounter initial: {}", counter.read().value());
    counter.write().increment();
    counter.write().increment();
    println!(
        "ManualCounter after 2 increments: {}",
        counter.read().value()
    );

    let state = app.require_mutable::<DeferredState>()?;
    println!("\nDeferredState.label: {:?}", state.read().label);
    state.read().counter.write().increment();
    println!(
        "DeferredState.counter (via dependency): {}",
        state.read().counter.read().value()
    );
    println!("ManualCounter (should be 3): {}", counter.read().value());

    let app_state = app.require_mutable::<AppState>()?;
    println!(
        "\nAppState initial: requests={}, errors={}",
        app_state.read().requests,
        app_state.read().errors
    );
    app_state.write().requests += 5;
    app_state.write().errors += 1;
    println!(
        "AppState after mutation: requests={}, errors={}",
        app_state.read().requests,
        app_state.read().errors
    );

    let h1 = app.require_mutable::<AppState>()?;
    let h2 = app.require_mutable::<AppState>()?;
    assert!(Arc::ptr_eq(&h1, &h2));
    println!(
        "\nBoth handles point to the same Mutable<AppState>: {}",
        Arc::ptr_eq(&h1, &h2)
    );

    println!("\n=== Example complete ===");
    Ok(())
}
