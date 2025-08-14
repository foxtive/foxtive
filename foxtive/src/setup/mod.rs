#[cfg(feature = "cache")]
#[allow(unused_imports)]
use crate::cache::{contract::CacheDriverContract, Cache};
#[cfg(feature = "database")]
use crate::database::create_db_pool;
#[cfg(feature = "jwt")]
use crate::helpers::jwt::Jwt;
#[cfg(feature = "crypto")]
use crate::helpers::password::Password;
use crate::prelude::AppMessage;
#[cfg(feature = "rabbitmq")]
use crate::prelude::RabbitMQ;
#[cfg(feature = "redis")]
use crate::prelude::Redis;
#[cfg(feature = "rabbitmq")]
use crate::rabbitmq::conn::create_rmq_conn_pool;
#[cfg(feature = "redis")]
use crate::redis::conn::create_redis_conn_pool;
use crate::results::AppResult;
use crate::setup::state::{FoxtiveHelpers, FoxtiveState};
use crate::Environment;
use std::path::Path;
#[allow(unused_imports)]
use std::sync::Arc;
#[cfg(feature = "templating")]
use tera::Tera;
use tracing::{debug, info};

pub(crate) mod state;
pub mod trace;
mod trace_layers;

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

    pub env: Environment,

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

pub async fn make_state(setup: FoxtiveSetup) -> AppResult<FoxtiveState> {
    debug!("Initializing Foxtive state for app: {}", setup.app_name);
    let foxtive = create_state(setup).await?;

    crate::FOXTIVE.set(foxtive.clone()).map_err(|_| {
        AppMessage::InternalServerErrorMessage("Failed to set global Foxtive state").ae()
    })?;

    debug!("Foxtive state initialized successfully");

    Ok(foxtive)
}

async fn create_state(setup: FoxtiveSetup) -> AppResult<FoxtiveState> {
    debug!("Creating helpers for app: {}", setup.app_code);
    let helpers = make_helpers(&setup);

    let env_prefix = setup.env_prefix;

    #[cfg(feature = "database")]
    let database_pool = {
        debug!("Initializing database pool");
        create_db_pool(setup.db_config)
    }?;

    #[cfg(feature = "redis")]
    let (redis, redis_pool) = {
        debug!("Initializing Redis connection pool");

        let redis_pool = create_redis_conn_pool(setup.redis_config)?;
        let redis = Arc::new(Redis::new(redis_pool.clone()));

        (redis, redis_pool)
    };

    #[cfg(feature = "rabbitmq")]
    let (rabbitmq, rabbitmq_pool) = {
        debug!("Initializing RabbitMQ connection pool");
        let rabbitmq_pool = create_rmq_conn_pool(setup.rmq_config).await?;

        let rmq = Arc::new(tokio::sync::Mutex::new(
            RabbitMQ::new(rabbitmq_pool.clone()).await?,
        ));

        (rmq, rabbitmq_pool)
    };

    #[cfg(feature = "templating")]
    let tera_templating = {
        debug!(
            "Initializing Tera templating engine from: {}",
            setup.template_directory
        );

        Tera::new(&setup.template_directory)
    }?;

    #[cfg(feature = "cache")]
    let cache_driver = {
        debug!("Setting up cache driver");
        match setup.cache_driver_setup {
            #[cfg(feature = "cache-redis")]
            CacheDriverSetup::Redis(setup_fn) => {
                debug!("Using Redis cache driver");
                setup_fn(redis.clone())
            }
            #[cfg(feature = "cache-filesystem")]
            CacheDriverSetup::Filesystem(setup_fn) => {
                debug!("Using Filesystem cache driver");
                setup_fn()
            }
            #[cfg(feature = "cache-in-memory")]
            CacheDriverSetup::InMemory(setup_fn) => {
                debug!("Using In-Memory cache driver");
                setup_fn()
            }
        }
    };

    debug!("All components initialized, creating final state");

    Ok(FoxtiveState {
        helpers,

        env: setup.env,

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
    })
}

#[allow(unused_variables)]
fn make_helpers(setup: &FoxtiveSetup) -> FoxtiveHelpers {
    #[cfg(feature = "crypto")]
    let pwd_helper = {
        debug!("Creating password helper");
        Password::new(setup.app_key.clone())
    };

    #[cfg(feature = "jwt")]
    let jwt_helper = {
        debug!(
            "Creating JWT helper with {}s token lifetime",
            setup.jwt_token_lifetime
        );
        Jwt::new(
            setup.jwt_iss_public_key.clone(),
            setup.private_key.clone(),
            setup.jwt_token_lifetime,
        )
    };

    FoxtiveHelpers {
        #[cfg(feature = "jwt")]
        jwt: Arc::new(jwt_helper),
        #[cfg(feature = "crypto")]
        password: Arc::new(pwd_helper),
    }
}

pub fn load_environment_variables(service: &str) {
    info!(
        "log level: {:?}",
        std::env::var("RUST_LOG").unwrap_or(String::from("info"))
    );
    info!("root directory: {service:?}");

    // load project level .env
    let path = format!("apps/{service}/.env");
    debug!("Attempting to load env file: {}", path);
    dotenv::from_filename(path).ok();

    // load project level .env
    if Path::new(".env").exists() {
        info!("loading env file: .env");
        dotenv::from_filename(".env").ok();
    }

    // load project level .env.main
    if Path::new(".env.main").exists() {
        info!("loading env file: .env.main");
        dotenv::from_filename(".env.main").ok();
    }

    // load service-specific env
    let filename = format!(".env.{service}");
    if Path::new(filename.as_str()).exists() {
        info!("loading env file: {filename}");
        dotenv::from_filename(filename).ok();
    }
}
