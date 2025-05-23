use crate::prelude::AppResult;
use anyhow::Error;
use deadpool_lapin::{Manager, Pool};
use lapin::ConnectionProperties;

pub async fn create_rmq_conn_pool(dsn: &str, pool_max_size: usize) -> AppResult<Pool> {
    let manager = Manager::new(dsn, ConnectionProperties::default());

    Pool::builder(manager)
        .max_size(pool_max_size)
        .build()
        .map_err(Error::msg)
}
