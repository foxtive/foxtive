//! # Setup Module
//!
//! Application bootstrap utilities: environment variable loading, panic handling,
//! and tracing configuration.
//!
//! ## Overview
//!
//! - [`App::load_env_files`](crate::App::load_env_files) - the public entry point for
//!   loading `.env` files (fail-fast, explicit paths). Returns a prefix-bound
//!   [`ScopedEnv`](crate::ScopedEnv) for immediate reads. The builder equivalents are
//!   [`AppBuilder::env_file`](crate::AppBuilder::env_file) /
//!   [`AppBuilder::env_files`](crate::AppBuilder::env_files), loaded at `build()`.
//! - [`panic`](mod@panic) - Environment-aware panic hook installation
//!   (also available via [`AppBuilder::panic_hook`](crate::AppBuilder::panic_hook)).
//! - `trace` - Tracing subscriber setup with configurable filters and formatters.
//!   (Requires the `tracing-setup` feature)
//!
//! ## Env File Layering
//!
//! Layering is simply **the order you pass paths**. `dotenvy` never overwrites
//! already-set variables, so the *first* file that sets a key wins - pass the
//! most-specific file first:
//!
//! ```rust,no_run
//! use foxtive::App;
//!
//! # fn run() -> foxtive::results::AppResult<()> {
//! let env = App::load_env_files("MYAPP", [
//!     "apps/my-service/.env", // service-specific (highest precedence)
//!     ".env",                 // project root
//!     ".env.main",             // shared overrides
//! ])?;
//! # Ok(())
//! # }
//! ```
//!
//! Every listed file must exist - a missing or unreadable file returns an error
//! naming the path.

use tracing::{debug, info, warn};

#[cfg(feature = "cache-redis")]
use crate::redis::Redis;
#[cfg(feature = "cache")]
use std::sync::Arc;

pub mod panic;

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
/// `dotenvy` never overwrites already-set variables, so the first file that sets
/// a key wins. Missing files are silently skipped.
/// If a file exists but fails to parse, a warning is logged.
///
/// # Deprecation Note
///
/// Crate-private as of 1.4: use [`App::load_env_files`](crate::App::load_env_files)
/// (fail-fast, explicit paths, returns a [`ScopedEnv`](crate::ScopedEnv)) or the
/// builder's `.env_file()` / `.env_files()` instead.
#[allow(dead_code)] // retained for one release; no internal callers
pub(crate) fn load_environment_variables(service: &str) {
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
