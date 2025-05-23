pub use deadpool::managed::QueueMode;
pub use deadpool_lapin::{PoolConfig, Timeouts};

pub struct RedisConfig {
    pub(crate) dsn: String,
    pub(crate) pool_config: PoolConfig,
}

impl RedisConfig {
    pub fn create(dsn: &str) -> Self {
        Self {
            dsn: dsn.to_string(),
            pool_config: PoolConfig::default(),
        }
    }

    pub fn pool_config(mut self, pool_config: PoolConfig) -> Self {
        self.pool_config = pool_config;
        self
    }
}
