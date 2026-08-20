use std::time::Duration;
use zeroize::Zeroizing;

use crate::enums::AppMessage;
use crate::results::AppResult;

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
///     .connection_timeout(Duration::from_secs(10))
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
    pub(crate) connection_timeout: Duration,
}

impl DbConfig {
    pub fn create(dsn: &str) -> Self {
        Self {
            dsn: Zeroizing::new(dsn.to_string()),
            max_size: 10,
            min_idle: None,
            test_on_check_out: true,
            idle_timeout: Some(Duration::from_secs(10 * 60)),
            max_lifetime: Some(Duration::from_secs(30 * 60)),
            connection_timeout: Duration::from_secs(30),
        }
    }

    /// Sets the maximum number of connections managed by the pool.
    ///
    /// Defaults to 10.
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
    /// Defaults to 30 seconds.
    ///
    /// # Validation
    /// Invalid values (e.g., zero Duration) are caught by [`validate()`](Self::validate).
    pub fn connection_timeout(mut self, connection_timeout: Duration) -> Self {
        self.connection_timeout = connection_timeout;
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
        if self.connection_timeout == Duration::ZERO {
            return Err(AppMessage::Infrastructure {
                message: "Database connection_timeout must not be zero".into(),
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
            .connection_timeout(Duration::from_secs(5));

        assert_eq!(config.max_size, 20);
        assert_eq!(config.min_idle, Some(5));
        assert!(!config.test_on_check_out);
        assert_eq!(config.max_lifetime, Some(Duration::from_secs(600)));
        assert_eq!(config.idle_timeout, Some(Duration::from_secs(120)));
        assert_eq!(config.connection_timeout, Duration::from_secs(5));
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
        let config =
            DbConfig::create("postgres://localhost/db").connection_timeout(Duration::from_secs(0));
        assert!(config.validate().is_err());
    }
}
