use std::fmt::{Debug, Formatter};
#[allow(unused_imports)]
use std::sync::Arc;

use crate::Environment;
#[cfg(feature = "jwt")]
use crate::helpers::jwt::Jwt;
#[cfg(feature = "crypto")]
use crate::helpers::password::Password;
#[cfg(feature = "rabbitmq")]
use crate::rabbitmq::RabbitMQ;
#[cfg(feature = "redis")]
use crate::redis::Redis;
#[cfg(feature = "templating")]
use tera::{Context, Tera};

#[derive(Clone)]
pub struct FoxtiveState {
    pub env: Environment,
    pub app_code: String,
    pub app_name: String,
    pub app_key: String,
    pub app_private_key: String,
    pub app_public_key: String,
    pub app_env_prefix: String,

    #[cfg(feature = "database")]
    pub(crate) database: crate::database::DBPool,

    #[cfg(feature = "templating")]
    pub(crate) tera: Tera,

    #[cfg(feature = "redis")]
    pub(crate) redis_pool: deadpool_redis::Pool,
    #[cfg(feature = "redis")]
    pub(crate) redis: Arc<Redis>,

    #[cfg(feature = "rabbitmq")]
    pub rabbitmq_pool: deadpool_lapin::Pool,
    #[cfg(feature = "rabbitmq")]
    pub rabbitmq: Arc<tokio::sync::Mutex<RabbitMQ>>,

    /// authentication issuer public key
    #[cfg(feature = "jwt")]
    pub jwt_iss_public_key: String,

    /// authentication token lifetime (in minutes)
    #[cfg(feature = "jwt")]
    pub jwt_token_lifetime: i64,

    #[cfg(feature = "cache")]
    pub cache: Arc<crate::cache::Cache>,

    pub helpers: FoxtiveHelpers,
}

#[derive(Clone)]
pub struct FoxtiveHelpers {
    #[cfg(feature = "jwt")]
    pub jwt: Arc<Jwt>,
    #[cfg(feature = "crypto")]
    pub password: Arc<Password>,
}

impl FoxtiveState {
    #[cfg(feature = "database")]
    pub fn database(&self) -> &crate::database::DBPool {
        &self.database
    }

    #[cfg(feature = "redis")]
    pub fn redis(&self) -> Arc<Redis> {
        self.redis.clone()
    }

    #[cfg(feature = "rabbitmq")]
    pub fn rabbitmq(&self) -> Arc<tokio::sync::Mutex<RabbitMQ>> {
        Arc::clone(&self.rabbitmq)
    }

    pub fn title(&self, text: &str) -> String {
        format!("{} - {}", text, self.app_name)
    }

    #[cfg(feature = "templating")]
    pub fn render(&self, mut file: String, context: Context) -> crate::results::AppResult<String> {
        if !file.ends_with(".tera.html") {
            file.push_str(".tera.html");
        }

        self.tera.render(&file, &context).map_err(crate::Error::msg)
    }
}

impl Debug for FoxtiveState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("application state")
    }
}
