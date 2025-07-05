use std::sync::OnceLock;

pub mod enums;
pub mod results;

#[cfg(feature = "redis")]
pub mod redis;

#[cfg(feature = "cache")]
pub mod cache;
#[cfg(feature = "database")]
pub mod database;
pub mod env_logger;
pub mod ext;
mod ext_impl;
pub mod helpers;
#[cfg(feature = "rabbitmq")]
pub mod rabbitmq;
pub mod setup;
pub mod tokio;

pub static FOXTIVE: OnceLock<FoxtiveState> = OnceLock::new();

pub use crate::setup::state::{FoxtiveHelpers, FoxtiveState};
pub use anyhow::Error;

pub mod prelude {
    pub use crate::enums::app_message::AppMessage;
    pub use crate::helpers::once_lock::OnceLockHelper;
    #[cfg(feature = "rabbitmq")]
    pub use crate::rabbitmq::RabbitMQ;
    #[cfg(feature = "redis")]
    pub use crate::redis::Redis;
    pub use crate::results::{AppResult, app_result::IntoAppResult};
}
