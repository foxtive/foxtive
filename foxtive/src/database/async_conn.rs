//! Async database connection pool creation using diesel_async + deadpool.

use crate::database::config::DbConfig;
use crate::results::AppResult;
use deadpool::Runtime;
use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::pooled_connection::deadpool::Pool;

/// Type alias for the async connection pool.
pub type AsyncDBPool = Pool<AsyncPgConnection>;

/// Create an async database connection pool from the given configuration.
///
/// Uses `diesel_async::AsyncDieselConnectionManager` with `deadpool::Pool`
/// for async PostgreSQL connection management.
///
/// The following `DbConfig` fields are honoured by this builder:
/// - `max_size` → pool capacity
/// - `connection_timeout` → deadpool `wait_timeout`
/// - `create_timeout` → deadpool `create_timeout`
/// - `recycle_timeout` → deadpool `recycle_timeout`
/// - `queue_mode` → deadpool `queue_mode`
///
/// A `Runtime::Tokio1` is always attached so that timeout enforcement works
/// regardless of whether the caller already has a runtime on the current
/// thread (this also fixes the "Timeouts require a runtime" build error).
pub fn create_async_db_pool(config: DbConfig) -> AppResult<AsyncDBPool> {
    let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new(config.dsn.to_string());

    let pool = Pool::builder(manager)
        .max_size(config.max_size as usize)
        .runtime(Runtime::Tokio1)
        .wait_timeout(config.connection_timeout)
        .create_timeout(config.create_timeout)
        .recycle_timeout(config.recycle_timeout)
        .queue_mode(config.queue_mode.into())
        .build()
        .map_err(|e| crate::enums::AppMessage::Infrastructure {
            message: format!("Async database pool creation failed: {e}"),
            source: None,
        })?;

    Ok(pool)
}
