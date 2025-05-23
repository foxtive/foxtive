use crate::redis::RedisConfig;
use crate::results::AppResult;
use anyhow::Error;
use deadpool_redis::{Manager, Pool};
use redis::Client;

pub fn create_redis_connection(dsn: &str) -> AppResult<Client> {
    Client::open(dsn).map_err(Error::msg)
}

pub fn create_redis_conn_pool(config: RedisConfig) -> AppResult<Pool> {
    let manager = Manager::new(config.dsn)?;

    Pool::builder(manager)
        .config(config.pool_config)
        .build()
        .map_err(Error::msg)
}
