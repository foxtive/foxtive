#[cfg(feature = "cache")]
#[allow(unused_imports)]
use crate::cache::{contract::CacheDriverContract, Cache};
#[cfg(feature = "database")]
use crate::database::DBPool;
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
#[cfg(feature = "database")]
use diesel::r2d2::ConnectionManager;
#[cfg(feature = "database")]
use diesel::{r2d2, PgConnection};
use log::info;
use std::path::Path;
#[allow(unused_imports)]
use std::sync::Arc;
use std::{env, fs};
#[cfg(feature = "templating")]
use tera::Tera;

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
    pub auth_iss_public_key: String,

    #[cfg(feature = "templating")]
    pub template_directory: String,

    #[cfg(feature = "rabbitmq")]
    pub rmq_dsn: String,
    #[cfg(feature = "rabbitmq")]
    pub rmq_pool_max_size: usize,

    #[cfg(feature = "redis")]
    pub redis_dsn: String,
    #[cfg(feature = "redis")]
    pub redis_pool_max_size: usize,

    #[cfg(feature = "cache")]
    pub cache_driver_setup: CacheDriverSetup,
}

pub async fn make_app_state(setup: FoxtiveSetup) -> FoxtiveState {
    let app = create_app_state(setup).await;
    crate::FOXTIVE
        .set(app.clone())
        .expect("Failed to set global Foxtive state");
    app
}

async fn create_app_state(setup: FoxtiveSetup) -> FoxtiveState {
    let helpers = make_helpers(&setup.env_prefix, &setup);
    let env_prefix = setup.env_prefix;

    #[cfg(feature = "database")]
    let database_pool = establish_database_connection(&env_prefix);

    #[cfg(feature = "redis")]
    let redis_pool = create_redis_conn_pool(&setup.rmq_dsn, setup.rmq_pool_max_size)
        .expect("Failed to initialize Redis connection pool.");

    #[cfg(feature = "redis")]
    let redis = Arc::new(Redis::new(redis_pool.clone()));

    // RabbitMQ
    #[cfg(feature = "rabbitmq")]
    let rabbitmq_pool = create_rmq_conn_pool(&setup.rmq_dsn, setup.rmq_pool_max_size)
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
        { Tera::new(&setup.template_directory).expect("Failed to initialize templating engine.") };

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

        app_code: env::var(format!("{}_APP_ID", env_prefix))
            .expect("APP_ID environment variable not set"),
        app_domain: env::var(format!("{}_APP_DOMAIN", env_prefix))
            .expect("APP_DOMAIN environment variable not set"),
        app_name: env::var(format!("{}_APP_NAME", env_prefix))
            .expect("APP_NAME environment variable not set"),
        app_desc: env::var(format!("{}_APP_DESC", env_prefix))
            .expect("APP_DESC environment variable not set"),
        app_help_email: env::var(format!("{}_APP_HELP_EMAIL", env_prefix))
            .expect("APP_HELP_EMAIL environment variable not set"),
        app_frontend_url: env::var(format!("{}_FRONTEND_ADDRESS", env_prefix))
            .expect("FRONTEND_ADDRESS environment variable not set"),

        app_private_key: setup.private_key,
        app_public_key: setup.public_key,
        app_key: env::var(format!("{}_APP_KEY", env_prefix))
            .expect("APP_KEY environment variable not set"),

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
        auth_iss_public_key: setup.auth_iss_public_key,

        #[cfg(feature = "jwt")]
        auth_token_lifetime: env::var(format!("{}_AUTH_TOKEN_LIFETIME", env_prefix))
            .unwrap()
            .parse()
            .unwrap(),

        #[cfg(feature = "cache")]
        cache: Arc::from(Cache::new(cache_driver)),
    }
}

pub fn get_server_host_config(env_prefix: &str) -> (String, u16, usize) {
    let host: String = env::var(format!("{}_SERVER_HOST", env_prefix))
        .expect("SERVER_HOST environment variable not set");
    let port: u16 = env::var(format!("{}_SERVER_PORT", env_prefix))
        .expect("SERVER_PORT environment variable not set")
        .parse()
        .expect("SERVER_PORT must be a valid u16 number");
    let workers: usize = env::var(format!("{}_SERVER_WORKERS", env_prefix))
        .expect("SERVER_WORKERS environment variable not set")
        .parse()
        .expect("SERVER_WORKERS must be a valid usize number");
    (host, port, workers)
}

#[allow(unused_variables)]
fn make_helpers(env_prefix: &str, setup: &FoxtiveSetup) -> FoxtiveHelpers {
    #[cfg(feature = "crypto")]
    let app_key =
        env::var(format!("{}_APP_KEY", env_prefix)).expect("APP_KEY environment variable not set");

    #[cfg(feature = "jwt")]
    let token_lifetime: i64 = env::var(format!("{}_AUTH_TOKEN_LIFETIME", env_prefix))
        .expect("AUTH_TOKEN_LIFETIME environment variable not set")
        .parse()
        .expect("AUTH_TOKEN_LIFETIME must be a valid i64 number");

    FoxtiveHelpers {
        #[cfg(feature = "jwt")]
        jwt: Arc::new(Jwt::new(
            setup.auth_iss_public_key.clone(),
            setup.private_key.clone(),
            token_lifetime,
        )),
        #[cfg(feature = "crypto")]
        password: Arc::new(Password::new(app_key)),
    }
}

#[cfg(feature = "database")]
pub fn establish_database_connection(env_prefix: &str) -> DBPool {
    let db_url: String = env::var(format!("{}_DATABASE_DSN", env_prefix)).unwrap();
    let manager = ConnectionManager::<PgConnection>::new(db_url);
    r2d2::Pool::builder()
        .build(manager)
        .expect("Failed to create database pool.")
}

pub fn load_config_file(file: &str) -> String {
    fs::read_to_string(format!("resources/config/{}", file)).expect("Failed to read config file")
}

pub fn load_environment_variables(service: &str) {
    info!(
        "log level: {:?}",
        env::var("RUST_LOG").unwrap_or(String::from("info"))
    );
    info!("root directory: {:?}", service);

    // load project level .env
    let path = format!("apps/{}/.env", service);
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
    let filename = format!(".env.{}", service);
    if Path::new(filename.as_str()).exists() {
        info!("loading env file: {}", filename);
        dotenv::from_filename(filename).ok();
    }
}
