use std::env;

use deadpool_lapin::{Manager, Pool};
use lapin::ConnectionProperties;

pub async fn establish_rabbit_connection_pool(env_prefix: &String) -> Pool {
    let dsn: String = env::var(format!("{}_RMQ_DSN", env_prefix)).unwrap();

    let manager = Manager::new(dsn, ConnectionProperties::default());
    Pool::builder(manager).max_size(10).build().unwrap()
}
