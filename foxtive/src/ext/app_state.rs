#[cfg(feature = "cache")]
use crate::cache::Cache;
#[cfg(feature = "database")]
use crate::database::ext::DatabaseConnectionExt;
#[cfg(feature = "database")]
use crate::prelude::AppResult;
use crate::{Environment, FOXTIVE};
use crate::{FoxtiveHelpers, FoxtiveState};
#[cfg(feature = "database")]
use diesel::r2d2::ConnectionManager;
#[cfg(feature = "database")]
use diesel::{PgConnection, r2d2};
#[allow(unused_imports)]
use std::sync::{Arc, OnceLock};

/// An extension trait for accessing the application's global state (`FoxtiveState`).
///
/// This trait provides a convenient, thread-safe interface to retrieve
/// application-wide resources and configuration, such as the environment,
/// database pool, and connection pools for other services.
pub trait AppStateExt {
    /// Returns an immutable reference to the global `FoxtiveState`.
    ///
    /// This method provides access to the application's singleton state, which is
    /// initialized globally and guaranteed to exist once the application is running.
    ///
    /// # Panics
    ///
    /// This function will panic if the global `FOXTIVE` state has not yet been
    /// initialized. This usually indicates a setup or startup error in the
    /// application's lifecycle, as the state is expected to be ready before this
    /// method is called.
    fn app(&self) -> &FoxtiveState {
        FOXTIVE.get().expect("Foxtive isn't initialized yet ")
    }

    /// Checks if the global `FOXTIVE` state has been initialized.
    ///
    /// This method does not cause initialization to occur. It only checks the
    /// current state of the underlying [`OnceLock`]. This is useful for
    /// conditional logic that depends on the state's availability without
    /// forcing its creation.
    ///
    /// # Returns
    ///
    /// Returns `true` if the global `FOXTIVE` state has been set, and `false`
    /// otherwise.
    fn is_initialized(&self) -> bool {
        FOXTIVE.get().is_some()
    }

    /// Returns the current application environment.
    ///
    /// This value is retrieved from the global `FoxtiveState`.
    fn env(&self) -> Environment {
        self.app().env
    }

    /// Returns the unique application code.
    ///
    /// This value is retrieved from the global `FoxtiveState`.
    fn app_code(&self) -> &String {
        &self.app().app_code
    }

    /// Returns a reference to the global helper functions.
    ///
    /// # Panics
    ///
    /// This function will panic if the global `FOXTIVE` state has not yet been
    /// initialized.
    fn helpers(&self) -> &FoxtiveHelpers {
        &FOXTIVE.get().unwrap().helpers
    }

    /// Returns a clone of the Redis connection pool.
    ///
    /// This method requires the `"redis"` feature to be enabled.
    ///
    /// # Panics
    ///
    /// This function will panic if the global `FOXTIVE` state has not yet been
    /// initialized.
    #[cfg(feature = "redis")]
    fn redis_pool(&self) -> deadpool_redis::Pool {
        self.app().redis_pool.clone()
    }

    /// Returns a reference to the Redis client wrapper.
    ///
    /// This method requires the `"redis"` feature to be enabled.
    ///
    /// # Panics
    ///
    /// This function will panic if the global `FOXTIVE` state has not yet been
    /// initialized.
    #[cfg(feature = "redis")]
    fn redis(&self) -> &crate::redis::Redis {
        &self.app().redis
    }

    /// Returns a clone of the RabbitMQ connection pool.
    ///
    /// This method requires the `"rabbitmq"` feature to be enabled.
    ///
    /// # Panics
    ///
    /// This function will panic if the global `FOXTIVE` state has not yet been
    /// initialized.
    #[cfg(feature = "rabbitmq")]
    fn rabbitmq_pool(&self) -> deadpool_lapin::Pool {
        self.app().rabbitmq_pool.clone()
    }

    /// Returns an `Arc` containing a `Mutex` locked RabbitMQ client wrapper.
    ///
    /// This method requires the `"rabbitmq"` feature to be enabled.
    ///
    /// # Panics
    ///
    /// This function will panic if the global `FOXTIVE` state has not yet been
    /// initialized.
    #[cfg(feature = "rabbitmq")]
    fn rabbitmq(&self) -> Arc<tokio::sync::Mutex<crate::prelude::RabbitMQ>> {
        self.app().rabbitmq.clone()
    }

    /// Returns a clone of the global `Cache` instance.
    ///
    /// This method requires the `"cache"` feature to be enabled.
    ///
    /// # Panics
    ///
    /// This function will panic if the global `FOXTIVE` state has not yet been
    /// initialized.
    #[cfg(feature = "cache")]
    fn cache(&self) -> Arc<Cache> {
        self.app().cache.clone()
    }

    /// Returns a reference to the database connection pool.
    ///
    /// This method requires the `"database"` feature to be enabled.
    ///
    /// # Panics
    ///
    /// This function will panic if the global `FOXTIVE` state has not yet been
    /// initialized.
    #[cfg(feature = "database")]
    fn db_pool(&self) -> &crate::database::DBPool {
        &self.app().database
    }

    /// Retrieves a single connection from the database pool.
    ///
    /// This method requires the `"database"` feature to be enabled.
    ///
    /// # Errors
    ///
    /// Returns an `AppResult` that contains an error if the database connection
    /// could not be retrieved from the pool.
    ///
    /// # Panics
    ///
    /// This function will panic if the global `FOXTIVE` state has not yet been
    /// initialized.
    #[cfg(feature = "database")]
    fn db_conn(&self) -> AppResult<r2d2::PooledConnection<ConnectionManager<PgConnection>>> {
        self.app().database.connection()
    }
}

impl AppStateExt for OnceLock<FoxtiveState> {}
