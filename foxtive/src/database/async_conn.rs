//! Async database connection pool creation using diesel_async + deadpool.

use crate::database::config::DbConfig;
use crate::results::AppResult;
use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::pooled_connection::deadpool::Pool;

/// Type alias for the async connection pool.
pub type AsyncDBPool = Pool<AsyncPgConnection>;

/// Create an async database connection pool from the given configuration.
///
/// Uses `diesel_async::AsyncDieselConnectionManager` with `deadpool::Pool`
/// for async PostgreSQL connection management.
pub fn create_async_db_pool(config: DbConfig) -> AppResult<AsyncDBPool> {
    let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new(config.dsn.to_string());

    let pool = Pool::builder(manager)
        .max_size(config.max_size as usize)
        .wait_timeout(Some(config.connection_timeout))
        .build()
        .map_err(|e| crate::enums::AppMessage::Infrastructure {
            message: format!("Async database pool creation failed: {e}"),
            source: None,
        })?;

    Ok(pool)
}
