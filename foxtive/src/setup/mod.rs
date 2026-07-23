//! # Setup Module
//!
//! Application bootstrap utilities: environment variable loading and tracing configuration.
//!
//! ## Overview
//!
//! - [`load_environment_variables()`] - Loads `.env` files from multiple conventional locations
//!   (project-level, service-specific, `.env.main`).
//! - [`trace`] - Tracing subscriber setup with configurable filters and formatters.
//!   (Requires the `tracing-setup` feature)
//!
//! ## Environment Loading Order
//!
//! The `load_environment_variables()` function loads `.env` files in this order:
//!
//! 1. `apps/{service}/.env` - Service-specific env
//! 2. `.env` - Root env
//! 3. `.env.main` - Main env override
//! 4. `.env.{service}` - Service-specific override
//!
//! Later files override earlier ones. Missing files are silently ignored;
//! malformed files are logged at warn level.
//!
//! ## Example
//!
//! ```rust,no_run
//! use foxtive::setup;
//!
//! // Load env vars before building the app
//! setup::load_environment_variables("my-service");
//! ```

use tracing::{debug, info, warn};

#[cfg(feature = "cache")]
use std::sync::Arc;
#[cfg(feature = "cache-redis")]
use crate::redis::Redis;

#[cfg(feature = "tracing-setup")]
pub mod trace;
#[cfg(feature = "tracing-setup")]
mod trace_layers;

#[cfg(feature = "tracing-setup")]
pub use trace::*;

#[cfg(feature = "cache")]
pub enum CacheDriverSetup {
    #[cfg(feature = "cache-redis")]
    Redis(fn(Arc<Redis>) -> Arc<dyn crate::cache::contract::CacheDriverContract>),
    #[cfg(feature = "cache-filesystem")]
    Filesystem(fn() -> Arc<dyn crate::cache::contract::CacheDriverContract>),
    #[cfg(feature = "cache-in-memory")]
    InMemory(fn() -> Arc<dyn crate::cache::contract::CacheDriverContract>),
}

/// Load environment variables from conventional `.env` file locations.
///
/// Checks the following paths in order (all optional, silently ignored if missing):
/// 1. `apps/{service}/.env`
/// 2. `.env` (project root)
/// 3. `.env.main`
/// 4. `.env.{service}` (service-specific)
///
/// Later files override earlier ones. Missing files are silently skipped.
/// If a file exists but fails to parse, a warning is logged.
///
/// # Example
///
/// ```rust,no_run
/// use foxtive::setup::load_environment_variables;
///
/// load_environment_variables("my-service");
/// ```
pub fn load_environment_variables(service: &str) {
    info!(
        "log level: {:?}",
        std::env::var("RUST_LOG").unwrap_or(String::from("info"))
    );
    info!("root directory: {service:?}");

    let paths = [
        format!("apps/{service}/.env"),
        ".env".to_string(),
        ".env.main".to_string(),
        format!(".env.{service}"),
    ];

    for path in &paths {
        debug!("Attempting to load env file: {}", path);
        if let Err(e) = dotenvy::from_filename(path) {
            // Only log if the file exists but failed to parse (not missing)
            if std::path::Path::new(path).exists() {
                warn!("Failed to parse env file '{}': {}", path, e);
            }
        }
    }
}
