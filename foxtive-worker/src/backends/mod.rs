pub mod contract;
pub mod memory;
pub mod dlq;
pub mod resilient;

#[cfg(feature = "rabbitmq")]
pub mod rabbitmq;

#[cfg(feature = "redis-stream")]
pub mod redis_stream;

pub use contract::MessageBackend;
pub use contract::ReceiveResult;
pub use memory::MemoryBackend;
pub use dlq::{DeadLetterQueueBackend, create_dlq_message};

#[cfg(feature = "rabbitmq")]
pub use rabbitmq::{RabbitMqBackend, RabbitMqConsumerConfig};

#[cfg(feature = "redis-stream")]
pub use redis_stream::{RedisStreamBackend, RedisStreamConsumerConfig};

// Re-export resilient backend types
pub use resilient::{ResilientBackend, ResilientBackendBuilder, ReconnectStrategy};
