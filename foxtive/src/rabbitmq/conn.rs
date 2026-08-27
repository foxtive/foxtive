use crate::prelude::{AppMessage, AppResult};
use crate::rabbitmq::config::RabbitmqConfig;
use deadpool_lapin::{Manager, Pool, Runtime};

pub async fn create_rmq_conn_pool(mut config: RabbitmqConfig) -> AppResult<Pool> {
    config.apply_timeouts();
    let runtime = async_rs::Runtime::tokio_current();
    let manager = Manager::new(
        config.dsn.to_string(),
        move || config.conn_props.clone(),
        runtime,
    );

    Pool::builder(manager)
        .config(config.pool_config)
        .runtime(Runtime::Tokio1)
        .build()
        .map_err(|e| AppMessage::Infrastructure {
            message: format!("RabbitMQ pool build error: {e}"),
            source: Some(Box::new(e)),
        })
}
