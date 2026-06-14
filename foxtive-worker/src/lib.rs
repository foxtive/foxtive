//! # Foxtive Worker
//!
//! A background worker framework for message processing, event-driven architectures,
//! and long-running background tasks.
//!
//! ## Overview
//!
//! Foxtive Worker provides a robust foundation for building message-processing workers
//! with features like:
//! - Manual message acknowledgment (ack/nack)
//! - Multiple backend support (RabbitMQ, Redis Streams, in-memory)
//! - Worker pools with load balancing
//! - Composable middleware pipeline
//! - Comprehensive observability and metrics
//! - HTTP health endpoints for Kubernetes integration
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use foxtive_worker::{Worker, ReceivedMessage};
//! use foxtive_worker::error::WorkerResult;
//! use async_trait::async_trait;
//!
//! struct MyWorker;
//!
//! #[async_trait]
//! impl Worker for MyWorker {
//!     fn id(&self) -> &str { "my-worker" }
//!     
//!     async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
//!         println!("Processing: {}", message.message.id);
//!         // Your processing logic here
//!         message.ack().await?;
//!         Ok(())
//!     }
//! }
//! ```
//!
//! ## Core Concepts
//!
//! ### Message Abstraction
//!
//! Messages are separated into pure data (`Message<T>`) and acknowledgment capability
//! (`AckHandle`), combined in `ReceivedMessage<T>` for safe concurrent processing.
//!
//! ### Two-Level Backoff System
//!
//! - **Worker Restart Backoff**: Controls worker restart after crashes (via `Worker::restart_backoff_strategy()`)
//! - **Message Retry Backoff**: Controls retry delays for failed messages (via RetryHandler middleware)
//!
//! These are independent strategies serving different purposes.

pub mod backends;
pub mod batch;
pub mod batch_processor;
pub mod builder;
pub mod dlq;
pub mod error;
pub mod health;
pub mod http;
pub mod message;
pub mod metrics;
pub mod middleware;
pub mod pool;
pub mod strategies;
pub mod stress;
pub mod worker;
mod message_properties;

// Re-exports for convenience
pub use crate::backends::{MemoryBackend, MessageBackend};
pub use crate::builder::WorkerPoolBuilder;
pub use crate::error::{WorkerError, WorkerResult};
pub use crate::message::{AckHandle, Message, MessageMetadata, ReceivedMessage};
pub use crate::pool::WorkerPool;
pub use crate::strategies::LoadBalancingStrategy;
pub use crate::message_properties::MessageProperties;
pub use crate::worker::{BackoffStrategy, Worker};

// Re-export resilient backend for all configurations
pub use crate::backends::{ReconnectStrategy, ResilientBackend, ResilientBackendBuilder};

#[cfg(feature = "rabbitmq")]
pub use crate::backends::{RabbitMqBackend, RabbitMqConsumerConfig};

#[cfg(feature = "redis-stream")]
pub use crate::backends::{RedisStreamBackend, RedisStreamConsumerConfig};

pub use crate::middleware::{
    MessageHandler, Middleware, MiddlewareChain, MiddlewareResult, ack_nack::AckNackMiddleware,
    batch::BatchMiddleware, circuit_breaker::CircuitBreakerMiddleware,
    processing_timeout::ProcessingTimeoutMiddleware, retry_handler::RetryHandler,
    tracing::TracingMiddleware,
};

#[cfg(feature = "rate-limit")]
pub use crate::middleware::rate_limit::RateLimitMiddleware;

#[cfg(feature = "metrics")]
pub use crate::metrics::MetricsCollector;
#[cfg(not(feature = "metrics"))]
pub use crate::metrics::NoOpMetrics as MetricsCollector;

pub use crate::batch::{
    BatchConfig, BatchHandler, BatchMetadata, BatchStatus, MessageBatch, ReceivedBatchMessage,
};
pub use crate::batch_processor::BatchProcessor;
pub use crate::dlq::{DeadLetterMessage, PoisonPillConfig, PoisonPillTracker};
pub use crate::health::{HealthCheck, HealthStatus, WorkerHealth, WorkerPoolHealth};

/// Common types and traits used throughout the crate.
pub mod prelude {
    pub use crate::backends::ReceiveResult;
    pub use crate::backends::{MemoryBackend, MessageBackend};
    pub use crate::backends::{ReconnectStrategy, ResilientBackend, ResilientBackendBuilder};
    pub use crate::builder::WorkerPoolBuilder;
    pub use crate::error::{WorkerError, WorkerResult};
    pub use crate::message::{
        AckHandle, Message, MessageMetadata, ReceivedJsonMessage, ReceivedMessage,
    };
    pub use crate::message_properties::MessageProperties;
    pub use crate::pool::WorkerPool;
    pub use crate::strategies::LoadBalancingStrategy;
    pub use crate::worker::{BackoffStrategy, Worker};

    #[cfg(feature = "rabbitmq")]
    pub use crate::backends::{RabbitMqBackend, RabbitMqConsumerConfig};

    #[cfg(feature = "redis-stream")]
    pub use crate::backends::{RedisStreamBackend, RedisStreamConsumerConfig};

    pub use crate::middleware::ack_nack::AckNackMiddleware;
    pub use crate::middleware::circuit_breaker::CircuitBreakerMiddleware;
    pub use crate::middleware::processing_timeout::ProcessingTimeoutMiddleware;
    #[cfg(feature = "rate-limit")]
    pub use crate::middleware::rate_limit::RateLimitMiddleware;
    pub use crate::middleware::retry_handler::RetryHandler;
    pub use crate::middleware::tracing::TracingMiddleware;
    pub use crate::middleware::{MessageHandler, Middleware, MiddlewareChain, MiddlewareResult};

    #[cfg(feature = "metrics")]
    pub use crate::metrics::MetricsCollector;
    #[cfg(not(feature = "metrics"))]
    pub use crate::metrics::NoOpMetrics as MetricsCollector;
    pub use crate::metrics::WorkerMetrics;

    pub use crate::health::{HealthCheck, HealthStatus, WorkerHealth, WorkerPoolHealth};
}
