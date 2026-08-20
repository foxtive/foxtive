//! Example demonstrating `#[service(all)]` for opt-out dependency declaration.
//!
//! `#[service(all)]` treats all fields as dependencies unless marked `#[foxtive(default)]`.

use foxtive::App;
use foxtive::container::Lazy;
use foxtive::lifecycle::Service;
use foxtive::prelude::AppResult;
use std::sync::Arc;

#[derive(Service, Default)]
struct CacheService;

#[derive(Service, Default)]
struct AuthService;

#[derive(Service, Default)]
struct NotificationService;

// Opt-in: every field needs #[dependency]
#[derive(Service, Default)]
struct UserServiceOptIn {
    #[dependency]
    cache: Arc<CacheService>,
    #[dependency]
    auth: Arc<AuthService>,
    #[dependency]
    notifications: Arc<NotificationService>,
}

impl UserServiceOptIn {
    fn describe(&self) -> String {
        let _ = (&self.cache, &self.auth, &self.notifications);
        "UserService (opt-in with #[dependency] on each field)".to_string()
    }
}

// Opt-out with #[service(all)]
#[derive(Service)]
#[service(all)]
struct UserServiceOptOut {
    cache: Arc<CacheService>,
    auth: Arc<AuthService>,
    notifications: Arc<NotificationService>,
}

impl UserServiceOptOut {
    fn describe(&self) -> String {
        let _ = (&self.cache, &self.auth, &self.notifications);
        "UserService (opt-out with #[service(all)], zero field annotations)".to_string()
    }
}

// Non-dep fields use #[foxtive(default)]
#[derive(Default)]
struct RequestCounter {
    count: u64,
}

#[derive(Service)]
#[service(all)]
struct UserServiceMixed {
    cache: Arc<CacheService>,
    auth: Arc<AuthService>,
    notifications: Arc<NotificationService>,
    #[foxtive(default)]
    counter: RequestCounter,
}

impl UserServiceMixed {
    fn describe(&self) -> String {
        let _ = (&self.cache, &self.auth, &self.notifications);
        format!(
            "UserService (mixed - 3 deps + 1 #[foxtive(default)] field, counter={})",
            self.counter.count
        )
    }
}

// Lazy<T> fields are auto-detected in both modes
#[derive(Service)]
#[service(all)]
struct OrderService {
    cache: Arc<CacheService>,
    user_service: Lazy<UserServiceOptOut>,
}

impl OrderService {
    fn describe(&self) -> String {
        let _ = &self.cache;
        let user = &*self.user_service;
        format!("OrderService → {}", user.describe())
    }
}

#[derive(Service)]
#[service(all, mutable)]
struct SessionService {
    cache: Arc<CacheService>,
    auth: Arc<AuthService>,
}

impl SessionService {
    fn describe(&self) -> String {
        let _ = (&self.cache, &self.auth);
        "SessionService (mutable + all deps)".to_string()
    }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    println!("=== #[service(all)] - Opt-out dependency declaration ===\n");

    let app = App::builder("Service All Demo", "SVCALL")
        .register_service::<CacheService>()
        .register_service::<AuthService>()
        .register_service::<NotificationService>()
        .register_service::<UserServiceOptIn>()
        .register_service::<UserServiceOptOut>()
        .register_service::<UserServiceMixed>()
        .register_service::<OrderService>()
        .register_service::<SessionService>()
        .build()
        .await?;

    let opt_in = app.require::<UserServiceOptIn>()?;
    println!("{}", opt_in.describe());

    let opt_out = app.require::<UserServiceOptOut>()?;
    println!("{}", opt_out.describe());

    let mixed = app.require::<UserServiceMixed>()?;
    println!("{}", mixed.describe());

    let order = app.require::<OrderService>()?;
    println!("{}", order.describe());
    println!(
        "OrderService.user_service filled: {}",
        order.user_service.is_filled()
    );

    let session = app.require_mutable::<SessionService>()?;
    println!("{}", session.read().describe());
    println!("\n=== Example complete ===");
    Ok(())
}
