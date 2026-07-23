use crate::prelude::AppMessage;
use crate::redis::config::RedisConfig;
use crate::results::AppResult;
use deadpool_redis::{Manager, Pool, Runtime};
use redis::Client;

pub fn create_redis_connection(dsn: &str) -> AppResult<Client> {
    Ok(Client::open(dsn)?)
}

pub fn create_redis_conn_pool(mut config: RedisConfig) -> AppResult<Pool> {
    config.apply_timeouts();
    let manager = Manager::new(config.dsn.to_string())?;

    Pool::builder(manager)
        .config(config.pool_config)
        .runtime(Runtime::Tokio1)
        .build()
        .map_err(|e| AppMessage::Infrastructure {
            message: format!("Redis pool build error: {e}"),
            source: Some(Box::new(e)),
        })
}
