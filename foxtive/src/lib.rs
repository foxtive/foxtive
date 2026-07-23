//! # Foxtive
//!
//! A production-grade Rust backend infrastructure crate providing battle-tested primitives
//! for HTTP services, async connection pooling, structured error handling, graceful shutdown,
//! health checking, secret zeroization, and extensible plugin architecture.
//!
//! ## Ecosystem
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │              APPLICATION LAYER                       │
//! │  (Your business logic, handlers, services)          │
//! └────────────────────┬────────────────────────────────┘
//!                      │
//!         ┌────────────▼────────────┐
//!         │  foxtive-axum / ntex    │
//!         │  (HTTP server adapters) │
//!         └────────────┬────────────┘
//!                      │
//!         ┌────────────▼────────────┐
//!         │    foxtive (CORE)       │
//!         │  - App DI container     │
//!         │  - AppMessage errors    │
//!         │  - Cache (multi-driver) │
//!         │  - Database (Diesel)    │
//!         │  - Redis / RabbitMQ     │
//!         │  - JWT / Crypto         │
//!         │  - Health checks        │
//!         │  - Lifecycle hooks      │
//!         │  - Plugin system        │
//!         └─────────────────────────┘
//! ```
//!
//! ## Quick Start
//!
//! ```no_run
//! use foxtive::App;
//! use foxtive::Environment;
//!
//! # async fn run() -> foxtive::results::AppResult<()> {
//! let app = App::builder("My Service", "MYAPP")
//!     .environment(Environment::Production)
//!     .app_key("secret-key")
//!     .build()
//!     .await?;
//!
//! assert_eq!(app.app_name(), "My Service");
//! # Ok(())
//! # }
//! ```
//!
//! ## Feature Flags
//!
//! | Feature | Description |
//! |---------|-------------|
//! | `database` | Diesel + r2d2 connection pool |
//! | `redis` | Redis client + deadpool pool |
//! | `rabbitmq` | Lapin + deadpool-lapin pool |
//! | `cache` | Cache abstraction (enable a driver below) |
//! | `cache-redis` | Redis cache driver |
//! | `cache-filesystem` | Filesystem cache driver |
//! | `cache-in-memory` | In-memory (DashMap) cache driver |
//! | `jwt` | JSON Web Token helpers |
//! | `crypto` | Argon2 password hashing |
//! | `hmac` | HMAC signing/verification |
//! | `base64` | Base64 encoding/decoding |
//! | `regex` | Fancy-regex utilities |
//! | `templating` | Tera template engine |
//! | `reqwest` | HTTP client helpers |
//! | `http` | URL-encoded query params |
//! | `openapi` | utoipa OpenAPI integration |
//! | `strum` | Enum string utilities |
//! | `html-sanitizer` | Ammonia HTML sanitization |
//! | `test-utils` | Testing helpers (`TestApp`) |

//!
//! ## Companion Crates
//!
//! - **foxtive-axum** / **foxtive-ntex** - HTTP server adapters with CORS, extractors, shutdown
//! - **foxtive-supervisor** - Task supervision with circuit breaker, distributed coordination
//! - **foxtive-worker** - Background job processing (RabbitMQ, Redis Streams)
//! - **foxtive-cron** - Cron scheduling with timezone support and persistence
//! - **foxtive-macros** - Proc macros for enum derivation

use std::collections::HashMap;

pub mod app;
pub mod config;
pub mod container;
pub mod enums;
pub mod events;
pub mod health;
pub mod lifecycle;
pub mod metrics;
pub mod results;
#[cfg(any(feature = "test-utils", test))]
pub mod testing;

#[cfg(feature = "redis")]
pub mod redis;

#[cfg(feature = "cache")]
pub mod cache;
#[cfg(any(feature = "database", feature = "database-async"))]
pub mod database;
mod env;
pub mod ext;

pub mod helpers;
#[cfg(feature = "http")]
pub mod http;
pub mod macros;
#[cfg(feature = "rabbitmq")]
pub mod rabbitmq;
pub mod setup;
pub mod tokio;


/// Structured validation errors: field name → list of messages.
///
/// A newtype wrapper around `HashMap<String, Vec<String>>` providing
/// convenience methods for building and querying validation errors.
///
/// # Example
///
/// ```
/// use foxtive::ValidationErrors;
///
/// let mut errors = ValidationErrors::new();
/// errors.add("email", "Invalid email format");
/// errors.add("email", "Email is required");
/// errors.add("password", "Password too short");
///
/// assert!(errors.has_field("email"));
/// assert_eq!(errors.field_count(), 2);
/// assert_eq!(errors.total_error_count(), 3);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationErrors(HashMap<String, Vec<String>>);

impl ValidationErrors {
    /// Create an empty `ValidationErrors`.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Add an error message for a field.
    pub fn add(&mut self, field: impl Into<String>, message: impl Into<String>) {
        self.0.entry(field.into()).or_default().push(message.into());
    }

    /// Returns `true` if there are no errors.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns `true` if the given field has errors.
    pub fn has_field(&self, field: &str) -> bool {
        self.0.contains_key(field)
    }

    /// Returns the error messages for a field, if any.
    pub fn field_errors(&self, field: &str) -> Option<&Vec<String>> {
        self.0.get(field)
    }

    /// Returns the number of fields with errors.
    pub fn field_count(&self) -> usize {
        self.0.len()
    }

    /// Returns the total number of error messages across all fields.
    pub fn total_error_count(&self) -> usize {
        self.0.values().map(|v| v.len()).sum()
    }

    /// Returns an iterator over `(field, messages)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.0.iter()
    }

    /// Insert a field with its error messages, replacing any existing entries.
    pub fn insert(&mut self, field: impl Into<String>, messages: Vec<String>) {
        self.0.insert(field.into(), messages);
    }

    /// Consumes self and returns the inner HashMap.
    pub fn into_inner(self) -> HashMap<String, Vec<String>> {
        self.0
    }
}

impl std::ops::Index<&str> for ValidationErrors {
    type Output = Vec<String>;

    /// Returns the error messages for a field.
    ///
    /// # Panics
    /// Panics if the field has no errors. Use [`field_errors()`](Self::field_errors)
    /// for a non-panicking alternative that returns `Option<&Vec<String>>`.
    #[track_caller]
    fn index(&self, field: &str) -> &Self::Output {
        &self.0[field]
    }
}

impl From<HashMap<String, Vec<String>>> for ValidationErrors {
    fn from(map: HashMap<String, Vec<String>>) -> Self {
        Self(map)
    }
}

impl From<ValidationErrors> for HashMap<String, Vec<String>> {
    fn from(errors: ValidationErrors) -> Self {
        errors.0
    }
}

impl IntoIterator for ValidationErrors {
    type Item = (String, Vec<String>);
    type IntoIter = std::collections::hash_map::IntoIter<String, Vec<String>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a ValidationErrors {
    type Item = (&'a String, &'a Vec<String>);
    type IntoIter = std::collections::hash_map::Iter<'a, String, Vec<String>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

pub use app::{App, AppBuilder, AppInit};
pub use app::DiError;


pub use ::http::StatusCode;

pub use env::Environment;
#[cfg(feature = "templating")]
pub use tera::{Tera, Context as TemplateContext};

pub mod prelude {
    pub use crate::app::{App, AppInit};
    pub use crate::app::DiError;
    pub use crate::container::{Lazy, Mutable};
    pub use crate::enums::AppMessage;
    pub use crate::events::{Event, EventHandler};
    pub use crate::lifecycle::{AsyncInit, FromApp, Service, ServiceHooks};

    #[cfg(feature = "rabbitmq")]
    pub use crate::rabbitmq::{IntoRmqError, RabbitMQ, RmqError, RmqResult};
    #[cfg(feature = "redis")]
    pub use crate::redis::Redis;
    pub use crate::results::{AppResult, app_result::IntoAppResult};
    pub use crate::tokio::Tokio;

    pub use crate::lazy;
}
