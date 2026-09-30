use std::time::Duration;
use zeroize::Zeroizing;

use crate::enums::AppMessage;
use crate::results::AppResult;

/// Returns deadpool's default pool size: `cpu_count * 2`.
///
/// Falls back to 10 when the OS cannot report the core count.
fn default_max_size() -> u32 {
    std::thread::available_parallelism()
        .map(|n| (n.get() * 2) as u32)
        .unwrap_or(10)
}

/// Queue mode for the async connection pool (deadpool-backed).
///
/// Determines the order in which connections are dequeued from the pool.
///
/// # Example
///
/// ```rust
/// use foxtive::database::PoolQueueMode;
///
/// let mode = PoolQueueMode::Lifo;
/// ```
#[cfg(feature = "database-async")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PoolQueueMode {
    /// Dequeue the connection that was least recently added (first in first out).
    ///
    /// This is the default and provides fair scheduling across all consumers.
    #[default]
    Fifo,
    /// Dequeue the connection that was most recently added (last in first out).
    ///
    /// Useful when you want to maximise cache warmth — the most recently used
    /// connection is more likely to still hold warm server-side state.
    Lifo,
}

#[cfg(feature = "database-async")]
impl From<PoolQueueMode> for deadpool::managed::QueueMode {
    fn from(mode: PoolQueueMode) -> Self {
        match mode {
            PoolQueueMode::Fifo => deadpool::managed::QueueMode::Fifo,
            PoolQueueMode::Lifo => deadpool::managed::QueueMode::Lifo,
        }
    }
}

/// Database connection pool configuration.
///
/// Uses a builder pattern to configure pool parameters, then validates
/// before creating the connection pool.
///
/// # Example
///
/// ```rust
/// use std::time::Duration;
/// use foxtive::database::DbConfig;
///
/// let config = DbConfig::create("postgres://user:pass@localhost/mydb")
///     .max_size(20)
///     .min_idle(Some(5))
///     .connection_timeout(Some(Duration::from_secs(10)))
///     .idle_timeout(Some(Duration::from_secs(300)));
///
/// // Validate before use
/// config.validate().expect("valid config");
/// ```
#[derive(Clone)]
pub struct DbConfig {
    pub(crate) dsn: Zeroizing<String>,
    pub(crate) max_size: u32,
    pub(crate) min_idle: Option<u32>,
    pub(crate) test_on_check_out: bool,
    pub(crate) max_lifetime: Option<Duration>,
    pub(crate) idle_timeout: Option<Duration>,
    pub(crate) connection_timeout: Option<Duration>,
    /// Timeout for establishing a new connection (async pool only).
    pub(crate) create_timeout: Option<Duration>,
    /// Timeout for recycling an existing connection (async pool only).
    pub(crate) recycle_timeout: Option<Duration>,
    /// Dequeue order for the async pool (async pool only).
    #[cfg(feature = "database-async")]
    pub(crate) queue_mode: PoolQueueMode,
}

impl DbConfig {
    pub fn create(dsn: &str) -> Self {
        Self {
            dsn: Zeroizing::new(dsn.to_string()),
            max_size: default_max_size(),
            min_idle: None,
            test_on_check_out: true,
            idle_timeout: Some(Duration::from_secs(10 * 60)),
            max_lifetime: Some(Duration::from_secs(30 * 60)),
            connection_timeout: None,
            create_timeout: None,
            recycle_timeout: None,
            #[cfg(feature = "database-async")]
            queue_mode: PoolQueueMode::default(),
        }
    }

    /// Sets the maximum number of connections managed by the pool.
    ///
    /// Defaults to `cpu_core_count * 2` (matching deadpool's default).
    ///
    /// # Validation
    /// Invalid values (e.g., 0) are caught by [`validate()`](Self::validate).
    pub fn max_size(mut self, max_size: u32) -> Self {
        self.max_size = max_size;
        self
    }

    /// Sets the minimum idle connection count maintained by the pool.
    ///
    /// If set, the pool will try to maintain at least this many idle
    /// connections at all times, while respecting the value of `max_size`.
    ///
    /// Defaults to `None` (equivalent to the value of `max_size`).
    pub fn min_idle(mut self, min_idle: Option<u32>) -> Self {
        self.min_idle = min_idle;
        self
    }

    /// If true, the health of a connection will be verified via a call to
    /// `ConnectionManager::is_valid` before it is checked out of the pool.
    ///
    /// Defaults to true.
    pub fn test_on_check_out(mut self, test_on_check_out: bool) -> Self {
        self.test_on_check_out = test_on_check_out;
        self
    }

    /// Sets the maximum lifetime of connections in the pool.
    ///
    /// If set, connections will be closed after existing for at most 30 seconds
    /// beyond this duration.
    ///
    /// If a connection reaches its maximum lifetime while checked out it will
    /// be closed when it is returned to the pool.
    ///
    /// Defaults to 30 minutes.
    ///
    /// # Validation
    /// Invalid values (e.g., zero Duration) are caught by [`validate()`](Self::validate).
    pub fn max_lifetime(mut self, max_lifetime: Option<Duration>) -> Self {
        self.max_lifetime = max_lifetime;
        self
    }

    /// Sets the idle timeout used by the pool.
    ///
    /// If set, connections will be closed after sitting idle for at most 30
    /// seconds beyond this duration.
    ///
    /// Defaults to 10 minutes.
    ///
    /// # Validation
    /// Invalid values (e.g., zero Duration) are caught by [`validate()`](Self::validate).
    pub fn idle_timeout(mut self, idle_timeout: Option<Duration>) -> Self {
        self.idle_timeout = idle_timeout;
        self
    }

    /// Sets the connection timeout used by the pool.
    ///
    /// Calls to `Pool::get` will wait this long for a connection to become
    /// available before returning an error.
    ///
    /// When `None`, no timeout is imposed — calls wait indefinitely.
    ///
    /// Defaults to `None` (matching deadpool's default).
    ///
    /// # Validation
    /// A zero `Duration` is rejected by [`validate()`](Self::validate).
    pub fn connection_timeout(mut self, connection_timeout: Option<Duration>) -> Self {
        self.connection_timeout = connection_timeout;
        self
    }

    /// Sets the timeout for establishing a new database connection.
    ///
    /// Applies to the async pool only (deadpool `create_timeout`).
    /// When `None`, no timeout is imposed on connection creation.
    ///
    /// Defaults to `None`.
    ///
    /// # Validation
    /// A zero `Duration` is rejected by [`validate()`](Self::validate).
    pub fn create_timeout(mut self, create_timeout: Option<Duration>) -> Self {
        self.create_timeout = create_timeout;
        self
    }

    /// Sets the timeout for recycling an existing database connection.
    ///
    /// Applies to the async pool only (deadpool `recycle_timeout`).
    /// When `None`, no timeout is imposed on connection recycling.
    ///
    /// Defaults to `None`.
    ///
    /// # Validation
    /// A zero `Duration` is rejected by [`validate()`](Self::validate).
    pub fn recycle_timeout(mut self, recycle_timeout: Option<Duration>) -> Self {
        self.recycle_timeout = recycle_timeout;
        self
    }

    /// Sets the queue mode for the async connection pool.
    ///
    /// Controls the order in which connections are dequeued:
    /// - [`PoolQueueMode::Fifo`] (default) — fair scheduling across consumers.
    /// - [`PoolQueueMode::Lifo`] — favours cache-warm connections.
    ///
    /// Applies to the async pool only; ignored by the blocking r2d2 pool.
    #[cfg(feature = "database-async")]
    pub fn queue_mode(mut self, queue_mode: PoolQueueMode) -> Self {
        self.queue_mode = queue_mode;
        self
    }

    /// Validate this configuration, returning a descriptive error if invalid.
    ///
    /// Checks:
    /// - DSN is not empty
    /// - `max_size` is > 0
    /// - `min_idle` does not exceed `max_size`
    /// - Duration fields are not zero
    pub fn validate(&self) -> AppResult<()> {
        if self.dsn.trim().is_empty() {
            return Err(AppMessage::Infrastructure {
                message: "Database DSN must not be empty".into(),
                source: None,
            });
        }
        if self.max_size == 0 {
            return Err(AppMessage::Infrastructure {
                message: "Database max_size must be greater than 0".into(),
                source: None,
            });
        }
        if let Some(min) = self.min_idle
            && min > self.max_size
        {
            return Err(AppMessage::Infrastructure {
                message: format!(
                    "Database min_idle ({}) must not exceed max_size ({})",
                    min, self.max_size
                ),
                source: None,
            });
        }
        if self.max_lifetime == Some(Duration::ZERO) {
            return Err(AppMessage::Infrastructure {
                message: "Database max_lifetime must not be zero".into(),
                source: None,
            });
        }
        if self.idle_timeout == Some(Duration::ZERO) {
            return Err(AppMessage::Infrastructure {
                message: "Database idle_timeout must not be zero".into(),
                source: None,
            });
        }
        if self.connection_timeout == Some(Duration::ZERO) {
            return Err(AppMessage::Infrastructure {
                message: "Database connection_timeout must not be zero".into(),
                source: None,
            });
        }
        if self.create_timeout == Some(Duration::ZERO) {
            return Err(AppMessage::Infrastructure {
                message: "Database create_timeout must not be zero".into(),
                source: None,
            });
        }
        if self.recycle_timeout == Some(Duration::ZERO) {
            return Err(AppMessage::Infrastructure {
                message: "Database recycle_timeout must not be zero".into(),
                source: None,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_config_passes_validation() {
        let config = DbConfig::create("postgres://localhost/db");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn empty_dsn_fails_validation() {
        let config = DbConfig::create("");
        assert!(config.validate().is_err());
    }

    #[test]
    fn whitespace_dsn_fails_validation() {
        let config = DbConfig::create("   ");
        assert!(config.validate().is_err());
    }

    #[test]
    fn min_idle_exceeding_max_size_fails() {
        let config = DbConfig::create("postgres://localhost/db")
            .max_size(5)
            .min_idle(Some(10));
        assert!(config.validate().is_err());
    }

    #[test]
    fn builder_chain_sets_values() {
        let config = DbConfig::create("postgres://localhost/db")
            .max_size(20)
            .min_idle(Some(5))
            .test_on_check_out(false)
            .max_lifetime(Some(Duration::from_secs(600)))
            .idle_timeout(Some(Duration::from_secs(120)))
            .connection_timeout(Some(Duration::from_secs(5)))
            .create_timeout(Some(Duration::from_secs(3)))
            .recycle_timeout(Some(Duration::from_secs(2)));

        assert_eq!(config.max_size, 20);
        assert_eq!(config.min_idle, Some(5));
        assert!(!config.test_on_check_out);
        assert_eq!(config.max_lifetime, Some(Duration::from_secs(600)));
        assert_eq!(config.idle_timeout, Some(Duration::from_secs(120)));
        assert_eq!(config.connection_timeout, Some(Duration::from_secs(5)));
        assert_eq!(config.create_timeout, Some(Duration::from_secs(3)));
        assert_eq!(config.recycle_timeout, Some(Duration::from_secs(2)));
    }

    #[test]
    fn max_size_zero_fails_validation() {
        let config = DbConfig::create("postgres://localhost/db").max_size(0);
        assert!(config.validate().is_err());
    }

    #[test]
    fn max_lifetime_zero_fails_validation() {
        let config =
            DbConfig::create("postgres://localhost/db").max_lifetime(Some(Duration::from_secs(0)));
        assert!(config.validate().is_err());
    }

    #[test]
    fn idle_timeout_zero_fails_validation() {
        let config =
            DbConfig::create("postgres://localhost/db").idle_timeout(Some(Duration::from_secs(0)));
        assert!(config.validate().is_err());
    }

    #[test]
    fn connection_timeout_zero_fails_validation() {
        let config = DbConfig::create("postgres://localhost/db")
            .connection_timeout(Some(Duration::from_secs(0)));
        assert!(config.validate().is_err());
    }

    #[test]
    fn connection_timeout_none_passes_validation() {
        let config = DbConfig::create("postgres://localhost/db").connection_timeout(None);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn create_timeout_zero_fails_validation() {
        let config = DbConfig::create("postgres://localhost/db")
            .create_timeout(Some(Duration::from_secs(0)));
        assert!(config.validate().is_err());
    }

    #[test]
    fn recycle_timeout_zero_fails_validation() {
        let config = DbConfig::create("postgres://localhost/db")
            .recycle_timeout(Some(Duration::from_secs(0)));
        assert!(config.validate().is_err());
    }

    #[test]
    fn create_timeout_none_passes_validation() {
        let config = DbConfig::create("postgres://localhost/db").create_timeout(None);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn recycle_timeout_none_passes_validation() {
        let config = DbConfig::create("postgres://localhost/db").recycle_timeout(None);
        assert!(config.validate().is_ok());
    }
}
