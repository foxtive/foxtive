use deadpool_lapin::PoolConfig;
use lapin::ConnectionProperties;

pub struct RabbitmqConfig {
    pub(crate) dsn: String,
    pub(crate) pool_config: PoolConfig,
    pub(crate) conn_props: ConnectionProperties,
}

impl RabbitmqConfig {
    pub fn create(dsn: &str) -> Self {
        Self {
            dsn: dsn.to_string(),
            pool_config: PoolConfig::default(),
            conn_props: ConnectionProperties::default(),
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
}
