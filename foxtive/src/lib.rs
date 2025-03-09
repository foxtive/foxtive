use std::sync::OnceLock;

use crate::app_state::FoxtiveState;

pub mod app_state;
pub mod enums;
pub mod results;

#[cfg(feature = "redis")]
pub mod redis;

pub mod helpers;

pub mod app_setup;
pub mod env_logger;
#[cfg(feature = "rabbitmq")]
pub mod rabbitmq;
pub mod tokio;
#[cfg(feature = "database")]
pub mod database;
#[cfg(feature = "redis")]
pub mod cache;

pub static FOXTIVE: OnceLock<FoxtiveState> = OnceLock::new();

pub use anyhow::Error;

pub mod prelude {
    pub use crate::app_state::FoxtiveState;
    pub use crate::enums::app_message::AppMessage;
    pub use crate::helpers::once_lock::OnceLockHelper;
    #[cfg(feature = "rabbitmq")]
    pub use crate::rabbitmq::RabbitMQ;
    #[cfg(feature = "redis")]
    pub use crate::redis::Redis;
    pub use crate::results::{app_result::IntoAppResult, AppResult};
    pub use crate::FOXTIVE;
}
