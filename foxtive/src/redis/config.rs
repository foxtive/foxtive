pub use deadpool::managed::QueueMode;
pub use deadpool_redis::{PoolConfig, Timeouts};

use crate::enums::AppMessage;
use crate::results::AppResult;
use std::time::Duration;
use zeroize::Zeroizing;

pub struct RedisConfig {
    pub(crate) dsn: Zeroizing<String>,
    pub(crate) pool_config: PoolConfig,
    pub(crate) wait_timeout: Option<Duration>,
    pub(crate) recycle_timeout: Option<Duration>,
}

impl RedisConfig {
    pub fn create(dsn: &str) -> Self {
        Self {
            dsn: Zeroizing::new(dsn.to_string()),
            pool_config: PoolConfig::default(),
            wait_timeout: None,
            recycle_timeout: None,
        }
    }

    pub fn pool_config(mut self, pool_config: PoolConfig) -> Self {
        self.pool_config = pool_config;
        self
    }

    /// Set the maximum time to wait for a connection from the pool.
    ///
    /// Defaults to 10 seconds if not set.
    pub fn wait_timeout(mut self, timeout: Duration) -> Self {
        self.wait_timeout = Some(timeout);
        self
    }

    /// Set the maximum time to wait for a connection recycle/health-check.
    ///
    /// Defaults to 2 seconds if not set.
    pub fn recycle_timeout(mut self, timeout: Duration) -> Self {
        self.recycle_timeout = Some(timeout);
        self
    }

    /// Validate this configuration, returning a descriptive error if invalid.
    ///
    /// Checks:
    /// - DSN is not empty
    pub fn validate(&self) -> AppResult<()> {
        if self.dsn.trim().is_empty() {
            return Err(AppMessage::Infrastructure {
                message: "Redis DSN must not be empty".into(),
                source: None,
            });
        }
        Ok(())
    }

    /// Apply configured timeouts to the pool config's timeout settings.
    pub(crate) fn apply_timeouts(&mut self) {
        let timeouts = Timeouts {
            wait: Some(self.wait_timeout.unwrap_or(Duration::from_secs(10))),
            recycle: Some(self.recycle_timeout.unwrap_or(Duration::from_secs(2))),
            ..self.pool_config.timeouts
        };
        self.pool_config.timeouts = timeouts;
    }
}
