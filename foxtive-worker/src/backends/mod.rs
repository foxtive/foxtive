pub mod contract;
pub mod dlq;
pub mod memory;
pub mod resilient;

#[cfg(feature = "rabbitmq")]
pub mod rabbitmq;

#[cfg(feature = "redis-stream")]
pub mod redis_stream;

pub use contract::MessageBackend;
pub use contract::ReceiveResult;
pub use dlq::{DeadLetterQueueBackend, create_dlq_message};
pub use memory::MemoryBackend;

#[cfg(feature = "rabbitmq")]
pub use rabbitmq::{RabbitMqBackend, RabbitMqConsumerConfig};

#[cfg(feature = "redis-stream")]
pub use redis_stream::{RedisStreamBackend, RedisStreamConsumerConfig};

// Re-export resilient backend types
pub use resilient::{ReconnectStrategy, ResilientBackend, ResilientBackendBuilder};
