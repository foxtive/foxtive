#[cfg(feature = "cache")]
#[allow(unused_imports)]
use crate::cache::{contract::CacheDriverContract, Cache};
#[cfg(feature = "database")]
use crate::database::create_db_pool;
#[cfg(feature = "jwt")]
use crate::helpers::jwt::Jwt;
#[cfg(feature = "crypto")]
use crate::helpers::password::Password;
#[cfg(feature = "rabbitmq")]
use crate::prelude::RabbitMQ;
#[cfg(feature = "redis")]
use crate::prelude::Redis;
#[cfg(feature = "rabbitmq")]
use crate::rabbitmq::conn::create_rmq_conn_pool;
#[cfg(feature = "redis")]
use crate::redis::conn::create_redis_conn_pool;
use crate::setup::state::{FoxtiveHelpers, FoxtiveState};
use std::fs;
use std::path::Path;
#[allow(unused_imports)]
use std::sync::Arc;
#[cfg(feature = "templating")]
use tera::Tera;
use tracing::info;

pub mod logger;
mod logger_layers;
pub(crate) mod state;

#[cfg(feature = "cache")]
pub enum CacheDriverSetup {
    #[cfg(feature = "cache-redis")]
    Redis(fn(Arc<Redis>) -> Arc<dyn CacheDriverContract>),
    #[cfg(feature = "cache-filesystem")]
    Filesystem(fn() -> Arc<dyn CacheDriverContract>),
    #[cfg(feature = "cache-in-memory")]
    InMemory(fn() -> Arc<dyn CacheDriverContract>),
}

pub struct FoxtiveSetup {
    pub env_prefix: String,
    pub private_key: String,
    pub public_key: String,
    pub app_key: String,
    pub app_code: String,
    pub app_name: String,

    #[cfg(feature = "jwt")]
    pub jwt_iss_public_key: String,
    #[cfg(feature = "jwt")]
    pub jwt_token_lifetime: i64,

    #[cfg(feature = "templating")]
    pub template_directory: String,

    #[cfg(feature = "database")]
    pub db_config: crate::database::DbConfig,

    #[cfg(feature = "rabbitmq")]
    pub rmq_config: crate::rabbitmq::config::RabbitmqConfig,

    #[cfg(feature = "redis")]
    pub redis_config: crate::redis::config::RedisConfig,

    #[cfg(feature = "cache")]
    pub cache_driver_setup: CacheDriverSetup,
}

pub async fn make_state(setup: FoxtiveSetup) -> FoxtiveState {
    let foxtive = create_state(setup).await;

    crate::FOXTIVE
        .set(foxtive.clone())
        .expect("Failed to set global Foxtive state");

    foxtive
}

async fn create_state(setup: FoxtiveSetup) -> FoxtiveState {
    let helpers = make_helpers(&setup);
    let env_prefix = setup.env_prefix;

    #[cfg(feature = "database")]
    let database_pool = create_db_pool(setup.db_config).expect("Failed to create database pool");

    #[cfg(feature = "redis")]
    let redis_pool = create_redis_conn_pool(setup.redis_config)
        .expect("Failed to initialize Redis connection pool.");

    #[cfg(feature = "redis")]
    let redis = Arc::new(Redis::new(redis_pool.clone()));

    // RabbitMQ
    #[cfg(feature = "rabbitmq")]
    let rabbitmq_pool = create_rmq_conn_pool(setup.rmq_config)
        .await
        .expect("Failed to initialize RabbitMQ connection pool.");

    #[cfg(feature = "rabbitmq")]
    let rabbitmq = Arc::new(tokio::sync::Mutex::new(
        RabbitMQ::new(rabbitmq_pool.clone())
            .await
            .expect("Failed to create RabbitMQ helper instance."),
    ));

    // templating
    #[cfg(feature = "templating")]
    let tera_templating =
        Tera::new(&setup.template_directory).expect("Failed to initialize templating engine.");

    #[cfg(feature = "cache")]
    let cache_driver = match setup.cache_driver_setup {
        #[cfg(feature = "cache-redis")]
        CacheDriverSetup::Redis(setup) => setup(redis.clone()),
        #[cfg(feature = "cache-filesystem")]
        CacheDriverSetup::Filesystem(setup) => setup(),
        #[cfg(feature = "cache-in-memory")]
        CacheDriverSetup::InMemory(setup) => setup(),
    };

    FoxtiveState {
        helpers,

        app_env_prefix: env_prefix.clone(),

        app_code: setup.app_code,
        app_name: setup.app_name,

        app_key: setup.app_key,
        app_public_key: setup.public_key,
        app_private_key: setup.private_key,

        #[cfg(feature = "redis")]
        redis_pool,
        #[cfg(feature = "redis")]
        redis,
        #[cfg(feature = "database")]
        database: database_pool,
        #[cfg(feature = "rabbitmq")]
        rabbitmq_pool,
        #[cfg(feature = "rabbitmq")]
        rabbitmq,
        #[cfg(feature = "templating")]
        tera: tera_templating,

        #[cfg(feature = "jwt")]
        jwt_iss_public_key: setup.jwt_iss_public_key,

        #[cfg(feature = "jwt")]
        jwt_token_lifetime: setup.jwt_token_lifetime,

        #[cfg(feature = "cache")]
        cache: Arc::from(Cache::new(cache_driver)),
    }
}

#[allow(unused_variables)]
fn make_helpers(setup: &FoxtiveSetup) -> FoxtiveHelpers {
    #[cfg(feature = "crypto")]
    let pwd_helper = Password::new(setup.app_key.clone());

    #[cfg(feature = "jwt")]
    let jwt_helper = Jwt::new(
        setup.jwt_iss_public_key.clone(),
        setup.private_key.clone(),
        setup.jwt_token_lifetime,
    );

    FoxtiveHelpers {
        #[cfg(feature = "jwt")]
        jwt: Arc::new(jwt_helper),
        #[cfg(feature = "crypto")]
        password: Arc::new(pwd_helper),
    }
}

pub fn load_config_file(file: &str) -> String {
    fs::read_to_string(format!("resources/config/{file}")).expect("Failed to read config file")
}

pub fn load_environment_variables(service: &str) {
    info!(
        "log level: {:?}",
        std::env::var("RUST_LOG").unwrap_or(String::from("info"))
    );
    info!("root directory: {service:?}");

    // load project level .env
    let path = format!("apps/{service}/.env");
    dotenv::from_filename(path).ok();

    // load project level .env.main
    if Path::new(".env").exists() {
        info!("loading env file: .env");
        dotenv::from_filename(".env").ok();
    }

    // load project level .env.main
    if Path::new(".env.main").exists() {
        info!("loading env file: .env.main");
        dotenv::from_filename(".env.main").ok();
    }

    // load project level .env.main
    let filename = format!(".env.{service}");
    if Path::new(filename.as_str()).exists() {
        info!("loading env file: {filename}");
        dotenv::from_filename(filename).ok();
    }
}
