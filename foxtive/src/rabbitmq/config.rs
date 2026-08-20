use lapin::ConnectionProperties;
use std::sync::Arc;

pub use deadpool::managed::QueueMode;
pub use deadpool_lapin::{PoolConfig, Timeouts};
use zeroize::Zeroizing;

use crate::enums::AppMessage;
use crate::prelude::RabbitMQ;
use crate::rabbitmq::{RabbitMQSetupFn, RmqResult};
use crate::results::AppResult;
use futures_util::future::BoxFuture;
use std::time::Duration;

#[derive(Clone)]
pub struct RabbitmqConfig {
    pub(crate) dsn: Zeroizing<String>,
    pub(crate) pool_config: PoolConfig,
    pub(crate) conn_props: ConnectionProperties,
    pub(crate) wait_timeout: Option<Duration>,
    pub(crate) recycle_timeout: Option<Duration>,
    pub(crate) setup_fn: Option<RabbitMQSetupFn>,
}

impl RabbitmqConfig {
    pub fn create(dsn: &str) -> Self {
        Self {
            dsn: Zeroizing::new(dsn.to_string()),
            pool_config: PoolConfig::default(),
            conn_props: ConnectionProperties::default(),
            wait_timeout: None,
            recycle_timeout: None,
            setup_fn: None,
        }
    }

    pub fn conn_props(mut self, conn_props: ConnectionProperties) -> Self {
        self.conn_props = conn_props;
        self
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

    /// Set a setup function that runs after every connection (initial or reconnection).
    ///
    /// Use this to declare exchanges, queues, and bindings that must exist
    /// whenever a fresh connection is established.
    pub fn setup_fn<F>(mut self, func: F) -> Self
    where
        F: Fn(RabbitMQ) -> BoxFuture<'static, RmqResult<()>> + Send + Sync + 'static,
    {
        self.setup_fn = Some(Arc::new(func));
        self
    }

    /// Validate this configuration, returning a descriptive error if invalid.
    ///
    /// Checks:
    /// - DSN is not empty
    pub fn validate(&self) -> AppResult<()> {
        if self.dsn.trim().is_empty() {
            return Err(AppMessage::Infrastructure {
                message: "RabbitMQ DSN must not be empty".into(),
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
