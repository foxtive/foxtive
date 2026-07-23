use crate::prelude::{AppMessage, AppResult};
use crate::rabbitmq::config::RabbitmqConfig;
use deadpool_lapin::{Manager, Pool, Runtime};

pub async fn create_rmq_conn_pool(mut config: RabbitmqConfig) -> AppResult<Pool> {
    config.apply_timeouts();
    let manager = Manager::new(config.dsn.to_string(), config.conn_props);

    Pool::builder(manager)
        .config(config.pool_config)
        .runtime(Runtime::Tokio1)
        .build()
        .map_err(|e| AppMessage::Infrastructure {
            message: format!("RabbitMQ pool build error: {e}"),
            source: Some(Box::new(e)),
        })
}
