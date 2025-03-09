#[allow(unused_imports)]
use std::sync::{Arc, OnceLock};

use crate::app_state::{AppHelpers, FoxtiveState};
#[cfg(feature = "redis")]
use crate::cache::Cache;
use crate::FOXTIVE;

pub trait OnceLockHelper {
    fn app(&self) -> &FoxtiveState {
        FOXTIVE.get().unwrap()
    }

    fn helpers(&self) -> &AppHelpers {
        &FOXTIVE.get().unwrap().helpers
    }

    fn front_url(&self, url: &str) -> String {
        self.app().frontend(url)
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

    #[cfg(feature = "redis")]
    fn cache(&self) -> Arc<Cache> {
        FOXTIVE.get().unwrap().cache.clone()
    }
}

impl OnceLockHelper for OnceLock<FoxtiveState> {}
