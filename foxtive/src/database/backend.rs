//! Backend-specific type aliases and re-exports.
//!
//! The active backend is selected via feature flags:
//! - `database-postgres` → PostgreSQL types (`PgConnection`, `Pg`, `AsyncPgConnection`)
//! - `database-mysql` → MySQL types (`MysqlConnection`, `Mysql`, `AsyncMysqlConnection`)
//!
//! ## Backward Compatibility
//!
//! When only `database` (without `database-postgres` or `database-mysql`) is enabled,
//! PostgreSQL is used as the default backend.
//!
//! ## Precedence
//!
//! When both `database-postgres` and `database-mysql` are enabled simultaneously,
//! PostgreSQL takes precedence.
//!
//! # Example
//!
//! ```ignore
//! use foxtive::database::backend::{DbConnection, AsyncDbConnection};
//!
//! // DbConnection resolves to PgConnection or MysqlConnection based on active feature.
//! // AsyncDbConnection resolves to AsyncPgConnection or AsyncMysqlConnection.
//! ```

// --- PostgreSQL backend (explicit or default when only `database` is enabled) ---
#[cfg(any(
    feature = "database-postgres",
    all(
        feature = "database",
        not(any(feature = "database-postgres", feature = "database-mysql"))
    )
))]
mod inner {
    pub use diesel::PgConnection;
    pub use diesel::pg::Pg;

    #[cfg(feature = "database-async")]
    pub use diesel_async::AsyncPgConnection;

    /// The blocking connection type for the active backend.
    pub type DbConnection = PgConnection;

    /// The async connection type for the active backend.
    #[cfg(feature = "database-async")]
    pub type AsyncDbConnection = AsyncPgConnection;
}

// --- MySQL backend (only when database-mysql is enabled AND database-postgres is NOT) ---
#[cfg(all(feature = "database-mysql", not(feature = "database-postgres")))]
mod inner {
    pub use diesel::MysqlConnection;
    pub use diesel::mysql::Mysql;

    #[cfg(feature = "database-async")]
    pub use diesel_async::AsyncMysqlConnection;

    /// The blocking connection type for the active backend.
    pub type DbConnection = MysqlConnection;

    /// The async connection type for the active backend.
    #[cfg(feature = "database-async")]
    pub type AsyncDbConnection = AsyncMysqlConnection;
}

pub use inner::*;
