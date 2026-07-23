//! # Database Module
//!
//! Provides database connection pools for PostgreSQL via Diesel.
//!
//! ## Feature Flags
//!
//! - `database` — Blocking Diesel + r2d2 connection pool ([`DBPool`])
//! - `database-async` — Async Diesel + deadpool connection pool ([`AsyncDBPool`])
//!
//! Both features can be enabled simultaneously for mixed workloads.
//!
//! ## Overview
//!
//! - [`DbConfig`] - Builder-style configuration for the connection pool (DSN, pool size, timeouts).
//! - [`pagination`] - Query pagination helpers with `Paginate` trait and `PageData` result type.
//! - [`ext`] - Extension traits for running queries against the pool.
//!
//! ## Example
//!
//! ```no_run
//! use foxtive::database::DbConfig;
//! use std::time::Duration;
//!
//! let config = DbConfig::create("postgres://user:pass@localhost/mydb")
//!     .max_size(20)
//!     .min_idle(Some(5))
//!     .connection_timeout(Duration::from_secs(10));
//!
//! // Validate before connecting
//! config.validate().expect("Invalid config");
//! ```
//!
//! ## Pagination
//!
//! ```ignore
//! use foxtive::database::pagination::Paginate;
//!
//! let page = users::table
//!     .paginate(1)
//!     .per_page(25)
//!     .load_and_count_pages::<User>(&mut conn)
//!     .expect("Failed to load page");
//!
//! println!("Page {} of {}", 1, page.total_pages);
//! for user in &page.records {
//!     println!("  {}", user.name);
//! }
//! ```

use serde::Serialize;

#[cfg(any(feature = "database", feature = "database-async"))]
mod config;

#[cfg(any(feature = "database", feature = "database-async"))]
pub use config::DbConfig;

#[cfg(any(feature = "database", feature = "database-async"))]
pub mod pagination;

#[cfg(any(feature = "database", feature = "database-async"))]
pub trait Model: Serialize {
    type Entity;
    fn into_shareable(self) -> Self::Entity;
}

#[cfg(feature = "database")]
mod conn;
#[cfg(feature = "database")]
pub mod ext;
#[cfg(feature = "database")]
mod ext_impl;

#[cfg(feature = "database")]
pub use conn::create_db_pool;

#[cfg(feature = "database")]
pub type DBPool = diesel::r2d2::Pool<diesel::r2d2::ConnectionManager<diesel::PgConnection>>;

#[cfg(feature = "database-async")]
mod async_conn;
#[cfg(feature = "database-async")]
pub mod async_ext;

#[cfg(feature = "database-async")]
pub use async_conn::{AsyncDBPool, create_async_db_pool};
