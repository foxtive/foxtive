use crate::prelude::AppResult;
use crate::rabbitmq::RabbitmqConfig;
use anyhow::Error;
use deadpool_lapin::{Manager, Pool};

pub async fn create_rmq_conn_pool(config: RabbitmqConfig) -> AppResult<Pool> {
    let manager = Manager::new(config.dsn, config.conn_props);

    Pool::builder(manager)
        .config(config.pool_config)
        .build()
        .map_err(Error::msg)
}
