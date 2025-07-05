use crate::FOXTIVE;
#[cfg(feature = "cache")]
use crate::cache::Cache;
#[cfg(feature = "database")]
use crate::database::ext::DatabaseConnectionExt;
#[cfg(feature = "database")]
use crate::prelude::AppResult;
use crate::{FoxtiveHelpers, FoxtiveState};
#[cfg(feature = "database")]
use diesel::r2d2::ConnectionManager;
#[cfg(feature = "database")]
use diesel::{PgConnection, r2d2};
#[allow(unused_imports)]
use std::sync::{Arc, OnceLock};

pub trait OnceLockHelper {
    fn app(&self) -> &FoxtiveState {
        FOXTIVE.get().unwrap()
    }

    fn helpers(&self) -> &FoxtiveHelpers {
        &FOXTIVE.get().unwrap().helpers
    }

    #[cfg(feature = "redis")]
    fn redis_pool(&self) -> deadpool_redis::Pool {
        self.app().redis_pool.clone()
    }

    #[cfg(feature = "redis")]
    fn redis(&self) -> &crate::redis::Redis {
        &FOXTIVE.get().unwrap().redis
    }

    #[cfg(feature = "rabbitmq")]
    fn rabbitmq_pool(&self) -> deadpool_lapin::Pool {
        self.app().rabbitmq_pool.clone()
    }

    #[cfg(feature = "rabbitmq")]
    fn rabbitmq(&self) -> Arc<tokio::sync::Mutex<crate::prelude::RabbitMQ>> {
        Arc::clone(&self.app().rabbitmq)
    }

    #[cfg(feature = "cache")]
    fn cache(&self) -> Arc<Cache> {
        FOXTIVE.get().unwrap().cache.clone()
    }

    #[cfg(feature = "database")]
    fn db_pool(&self) -> &crate::database::DBPool {
        &self.app().database
    }

    #[cfg(feature = "database")]
    fn db_conn(&self) -> AppResult<r2d2::PooledConnection<ConnectionManager<PgConnection>>> {
        self.app().database.connection()
    }
}

impl OnceLockHelper for OnceLock<FoxtiveState> {}
