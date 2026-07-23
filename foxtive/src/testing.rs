//! Testing utilities for creating `App` instances in tests.
//!
//! These helpers make it easy to create minimal [`App`] containers for unit
//! and integration tests without requiring real database connections, Redis, etc.
//!
//! # Example
//!
//! ```
//! use foxtive::testing::TestApp;
//!
//! #[tokio::test]
//! async fn test_my_service() {
//!     let app = TestApp::minimal().await;
//!     // Use app for testing...
//! }
//! ```

use std::sync::Arc;

use crate::app::AppBuilder;
use crate::lifecycle::ServiceInit;
use crate::prelude::AppResult;
use crate::App;
use crate::Environment;

/// Helper for creating [`App`] instances configured for testing.
///
/// Provides convenience methods to create minimal app containers
/// or pre-configured builders for test scenarios.
pub struct TestApp;

impl TestApp {
    /// Create a minimal `App` for testing (no real connections).
    ///
    /// The returned app has:
    /// - Environment set to `Local`
    /// - App name "test-app" and code "TEST"
    /// - No database, redis, rabbitmq, or cache connections
    pub async fn minimal() -> Arc<App> {
        App::builder("test-app", "TEST")
            .environment(Environment::Local)
            .build()
            .await
            .expect("Failed to build test app")
    }

    /// Create an [`AppBuilder`] pre-configured for tests.
    ///
    /// The builder has sensible test defaults but can be further
    /// customized before calling `.build().await`.
    pub fn builder() -> AppBuilder {
        App::builder("test-app", "TEST")
            .environment(Environment::Local)
    }

    /// Create a minimal `App` with a custom name.
    pub async fn named(name: &str, code: &str) -> Arc<App> {
        App::builder(name, code)
            .environment(Environment::Local)
            .build()
            .await
            .expect("Failed to build test app")
    }
}

impl App {
    /// Create a builder pre-configured for testing.
    ///
    /// Sets environment to `Local` and uses "TEST" as the app code.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use foxtive::App;
    ///
    /// # async fn run() {
    /// let app = App::test_builder("my-test")
    ///     .register(42i32)
    ///     .build()
    ///     .await
    ///     .unwrap();
    ///
    /// assert_eq!(*app.get::<i32>().unwrap(), 42);
    /// # }
    /// ```
    pub fn test_builder(name: &str) -> AppBuilder {
        App::builder(name, "TEST")
            .environment(Environment::Local)
    }

    /// Build a minimal app with only the given service and resolve it.
    ///
    /// Useful for unit-testing a single service in isolation.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use foxtive::App;
    /// use foxtive::lifecycle::ServiceInit;
    /// use foxtive::prelude::AppResult;
    ///
    /// struct MyService;
    /// impl ServiceInit for MyService {
    ///     async fn init(_app: &App) -> AppResult<Self> { Ok(Self) }
    /// }
    ///
    /// # async fn run() -> AppResult<()> {
    /// let svc = App::resolve_for_test::<MyService>().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn resolve_for_test<T: ServiceInit>() -> AppResult<Arc<T>> {
        let app = App::test_builder("test")
            .register_service::<T>()
            .build()
            .await?;
        app.require::<T>()
    }
}
