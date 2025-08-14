use std::time::Duration;

#[derive(Clone)]
pub struct DbConfig {
    pub(crate) dsn: String,
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
            dsn: dsn.to_string(),
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
    /// # Panics
    ///
    /// Panics if `max_size` is 0.
    pub fn max_size(mut self, max_size: u32) -> Self {
        assert!(max_size > 0, "max_size must be positive");
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
    /// # Panics
    ///
    /// Panics if `max_lifetime` is the zero `Duration`.
    pub fn max_lifetime(mut self, max_lifetime: Option<Duration>) -> Self {
        assert_ne!(
            max_lifetime,
            Some(Duration::from_secs(0)),
            "max_lifetime must be positive"
        );
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
    /// # Panics
    ///
    /// Panics if `idle_timeout` is the zero `Duration`.
    pub fn idle_timeout(mut self, idle_timeout: Option<Duration>) -> Self {
        assert_ne!(
            idle_timeout,
            Some(Duration::from_secs(0)),
            "idle_timeout must be positive"
        );
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
    /// # Panics
    ///
    /// Panics if `connection_timeout` is the zero duration
    pub fn connection_timeout(mut self, connection_timeout: Duration) -> Self {
        assert!(
            connection_timeout > Duration::from_secs(0),
            "connection_timeout must be positive"
        );
        self.connection_timeout = connection_timeout;
        self
    }
}
