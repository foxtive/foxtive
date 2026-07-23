use crate::database::config::DbConfig;
use crate::results::AppResult;
use diesel::r2d2::ConnectionManager;
use diesel::{PgConnection, r2d2};

pub fn create_db_pool(config: DbConfig) -> AppResult<crate::database::DBPool> {
    let manager = ConnectionManager::<PgConnection>::new(config.dsn.to_string());
    let mut builder = r2d2::Pool::builder()
        .max_size(config.max_size)
        .max_lifetime(config.max_lifetime)
        .min_idle(config.min_idle)
        .idle_timeout(config.idle_timeout)
        .connection_timeout(config.connection_timeout);
    
    // Wire up connection validation if enabled
    if config.test_on_check_out {
        builder = builder.test_on_check_out(true);
    }
    
    let pool = builder.build(manager)?;
    Ok(pool)
}
