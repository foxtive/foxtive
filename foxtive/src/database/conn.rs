use crate::database::config::DbConfig;
use crate::results::AppResult;
use anyhow::Error;
use diesel::r2d2::ConnectionManager;
use diesel::{r2d2, PgConnection};

pub fn create_db_pool(config: DbConfig) -> AppResult<crate::database::DBPool> {
    let manager = ConnectionManager::<PgConnection>::new(&config.dsn);
    r2d2::Pool::builder()
        .max_size(config.max_size)
        .max_lifetime(config.max_lifetime)
        .min_idle(config.min_idle)
        .idle_timeout(config.idle_timeout)
        .connection_timeout(config.connection_timeout)
        .build(manager)
        .map_err(Error::msg)
}
