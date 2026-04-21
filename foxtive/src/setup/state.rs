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

/// The shared application state.
///
/// This struct holds all the shared state for the application, including database connections,
/// template engines, and other services.
#[derive(Clone)]
pub struct FoxtiveState {
    /// The application's environment (e.g., `Development`, `Production`).
    pub env: Environment,
    /// The application's code name.
    pub app_code: String,
    /// The application's name.
    pub app_name: String,
    /// The application's secret key.
    pub app_key: String,
    /// The application's private key.
    pub app_private_key: String,
    /// The application's public key.
    pub app_public_key: String,
    /// The prefix for environment variables.
    pub app_env_prefix: String,

    #[cfg(feature = "database")]
    /// The database connection pool.
    pub(crate) database: crate::database::DBPool,

    #[cfg(feature = "templating")]
    /// The Tera template engine.
    pub(crate) tera: Tera,

    #[cfg(feature = "redis")]
    /// The Redis connection pool.
    pub(crate) redis_pool: deadpool_redis::Pool,
    #[cfg(feature = "redis")]
    /// The Redis client.
    pub(crate) redis: Arc<Redis>,

    #[cfg(feature = "rabbitmq")]
    /// The RabbitMQ connection pool.
    pub rabbitmq_pool: deadpool_lapin::Pool,
    #[cfg(feature = "rabbitmq")]
    /// The RabbitMQ client.
    pub rabbitmq: Arc<tokio::sync::Mutex<RabbitMQ>>,

    /// The public key for the JWT issuer.
    #[cfg(feature = "jwt")]
    pub jwt_iss_public_key: String,

    /// The lifetime of the JWT token in minutes.
    #[cfg(feature = "jwt")]
    pub jwt_token_lifetime: i64,

    #[cfg(feature = "cache")]
    /// The cache client.
    pub cache: Arc<crate::cache::Cache>,

    /// A collection of helper utilities.
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
    pub fn render(
        &self,
        file: impl Into<String>,
        context: &Context,
    ) -> crate::results::AppResult<String> {
        let mut file = file.into();
        if !file.ends_with(".tera.html") {
            file.push_str(".tera.html");
        }

        self.tera.render(&file, context).map_err(crate::Error::msg)
    }
}

impl Debug for FoxtiveState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("application state")
    }
}
