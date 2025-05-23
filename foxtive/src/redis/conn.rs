use crate::results::AppResult;
use anyhow::Error;
use deadpool_redis::{Manager, Pool};
use redis::Client;

pub fn create_redis_connection(dsn: &str) -> AppResult<Client> {
    Client::open(dsn).map_err(Error::msg)
}

pub fn create_redis_conn_pool(dsn: &str, pool_max_size: usize) -> AppResult<Pool> {
    let manager = Manager::new(dsn)?;

    Pool::builder(manager)
        .max_size(pool_max_size)
        .build()
        .map_err(Error::msg)
}
