use std::env;

use deadpool_redis::{Manager, Pool};
use redis::Client;

pub fn establish_redis_connection(env_prefix: &String) -> Client {
    let redis_url: String = env::var(format!("{}_REDIS_DSN", env_prefix)).unwrap();
    Client::open(redis_url).unwrap()
}

pub fn establish_redis_connection_pool(env_prefix: &String) -> Pool {
    let dsn: String = env::var(format!("{}_REDIS_DSN", env_prefix)).unwrap();

    let manager = Manager::new(dsn).unwrap();
    Pool::builder(manager).max_size(16).build().unwrap()
}
