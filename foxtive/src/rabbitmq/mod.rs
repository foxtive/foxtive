//! # RabbitMQ Module
//!
//! High-level RabbitMQ client with automatic reconnection, push-based consumers,
//! and pull-based consumer streams.
//!
//! ## Overview
//!
//! - [`RabbitMQ`] - Main client with publish, consume, and stream operations
//! - [`ConsumerStream`] - Async stream for pull-based message consumption
//! - [`ConsumerGuard`] - Lifecycle guard for consumer cleanup
//! - [`MessagePublisher`] - Trait for message publishing
//! - [`config`] - Connection pool configuration
//!
//! ## Features
//!
//! - Automatic reconnection with exponential backoff
//! - Push-based consumers with async handlers
//! - Pull-based consumers via `ConsumerStream`
//! - Configurable nack/requeue on failure
//! - Graceful shutdown with cancellation tokens
//!
//! ## Example
//!
//! ```no_run
//! use foxtive::prelude::RabbitMQ;
//!
//! # async fn example(pool: deadpool_lapin::Pool) {
//! let rmq = RabbitMQ::new(pool).await.unwrap();
//!
//! // Publish a message
//! rmq.publish("events", "user.created", b"{\"id\": 1}")
//!     .await
//!     .unwrap();
//!
//! // Consume messages (push-based)
//! rmq.consume("user_queue", "worker-1", |msg| async move {
//!     println!("Received: {:?}", msg.data());
//!     msg.ack().await.unwrap();
//!     Ok(())
//! }).await.unwrap();
//! # }
//! ```

use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use lapin::{Channel, ConnectionState};
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

pub use {
    lapin::message::{Delivery, DeliveryResult},
    lapin::types::*,
    lapin::{BasicProperties, ChannelState, ExchangeKind, options::*},
};

pub use crate::rabbitmq::error::{IntoRmqError, RmqError, RmqResult};

pub use crate::rabbitmq::message::Message;

pub mod config;
pub mod conn;
mod error;
mod message;
mod message_publisher;

pub use message_publisher::MessagePublisher;

pub type RabbitMQSetupFn = Arc<dyn Fn(RabbitMQ) -> BoxFuture<'static, RmqResult<()>> + Send + Sync>;

/// Async stream for pull-based message consumption.
///
/// Implements `futures_util::Stream` to allow receiving messages via `.next().await`.
/// The stream will continue until the consumer is cancelled or an error occurs.
pub struct ConsumerStream {
    inner: tokio_stream::wrappers::ReceiverStream<RmqResult<Message>>,
}

impl ConsumerStream {
    /// Receive the next message from the stream.
    ///
    /// Returns `None` when the stream is closed (consumer cancelled or error).
    pub async fn next(&mut self) -> Option<RmqResult<Message>> {
        use futures_util::StreamExt;
        self.inner.next().await
    }
}

// Implement Stream trait for compatibility with futures utilities
impl futures_util::Stream for ConsumerStream {
    type Item = RmqResult<Message>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // Delegate to ReceiverStream which correctly registers wakers
        tokio_stream::wrappers::ReceiverStream::poll_next(std::pin::Pin::new(&mut self.inner), cx)
    }
}

/// Guard that manages the lifecycle of a pull-based consumer.
///
/// When dropped, the guard will cancel the consumer and clean up resources.
/// Keep this alive for as long as you want to receive messages.
pub struct ConsumerGuard {
    handle: Option<tokio::task::JoinHandle<()>>,
    cancellation_token: CancellationToken,
}

impl ConsumerGuard {
    /// Cancel the consumer and await its completion.
    ///
    /// Signals cancellation and waits for the consumer task to finish.
    /// Use this for graceful shutdown where you need to ensure the
    /// consumer has fully stopped.
    pub async fn cancel_async(&mut self) {
        self.cancellation_token.cancel();
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
    }

    /// Cancel the consumer immediately (non-blocking).
    ///
    /// Signals cancellation and drops the task handle without awaiting.
    /// The consumer task may continue briefly after this returns.
    pub fn cancel(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.cancellation_token.cancel();
            drop(handle);
        }
    }
}

impl Drop for ConsumerGuard {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Internal state holding AMQP channels that may need reconnection.
///
/// Wrapped in `Arc<RwLock<...>>` so that concurrent publish/ack operations
/// can proceed in parallel (read lock) while reconnection acquires exclusive
/// access (write lock).
struct RabbitMQInner {
    publish_channel: Channel,
    consume_channel: Channel,
}

/// RabbitMQ client with automatic reconnection, push-based consumers,
/// and pull-based consumer streams.
///
/// # Example
///
/// ```no_run
/// use foxtive::prelude::RabbitMQ;
///
/// # async fn example(pool: deadpool_lapin::Pool) {
/// let rmq = RabbitMQ::new(pool).await.unwrap();
///
/// // Publish a message
/// rmq.publish("events", "user.created", b"{\"id\": 1}")
///     .await
///     .unwrap();
///
/// // Consume messages
/// rmq.consume("user_queue", "worker-1", |msg| async move {
///     println!("Received: {:?}", msg.data());
///     msg.ack().await.unwrap();
///     Ok(())
/// }).await.unwrap();
/// # }
/// ```
#[derive(Clone)]
pub struct RabbitMQ {
    conn_pool: deadpool_lapin::Pool,
    inner: Arc<tokio::sync::RwLock<RabbitMQInner>>,
    /// Controls reconnection behavior (atomic for lock-free reads).
    can_reconnect: Arc<AtomicBool>,
    /// Nack messages on handler error
    nack_on_failure: bool,
    /// Requeue failed messages
    requeue_on_failure: bool,
    /// Run handlers asynchronously
    execute_handler_asynchronously: bool,
    /// Max reconnection attempts (default: 1M)
    max_reconnection_attempts: usize,
    /// Initial reconnection delay (default: 1s)
    max_reconnection_delay: Duration,
    /// Default publish options and properties
    default_publish_options: BasicPublishOptions,
    #[allow(dead_code)]
    default_publish_props: BasicProperties,
    /// Default consume options
    default_consume_options: BasicConsumeOptions,
    /// Optional setup function after connection
    setup_fn: Option<RabbitMQSetupFn>,
    /// Operation timeout (default: 30s)
    operation_timeout: Duration,
    /// Health check interval in messages (default: 100, 0 to disable)
    health_check_interval: usize,
    /// QoS prefetch count (default: 10, 0 = unlimited)
    prefetch_count: u16,
    /// Graceful shutdown token
    cancellation_token: CancellationToken,
}

#[derive(Default)]
pub struct RabbitMQOptions {
    /// Nack messages on handler error (default: true)
    pub nack_on_failure: bool,
    /// Requeue failed messages (default: true)
    pub requeue_on_failure: bool,
    /// Run handlers asynchronously (default: true)
    pub execute_handler_asynchronously: bool,
}
impl RabbitMQ {
    const RETRY_DELAY: Duration = Duration::from_secs(2);

    /// Create new instance and connect
    pub async fn new(pool: deadpool_lapin::Pool) -> RmqResult<Self> {
        Self::new_opt(
            pool,
            RabbitMQOptions {
                nack_on_failure: true,
                requeue_on_failure: true,
                execute_handler_asynchronously: true,
            },
        )
        .await
    }

    /// Create from an existing pool.
    pub async fn new_from_pool(pool: deadpool_lapin::Pool) -> RmqResult<Self> {
        Self::new_opt(
            pool,
            RabbitMQOptions {
                nack_on_failure: true,
                requeue_on_failure: true,
                execute_handler_asynchronously: true,
            },
        )
        .await
    }

    pub async fn new_opt(pool: deadpool_lapin::Pool, opt: RabbitMQOptions) -> RmqResult<Self> {
        let connection = pool.get().await?;
        let publish_channel = connection.create_channel().await?;
        let consume_channel = connection.create_channel().await?;

        Ok(Self {
            setup_fn: None,
            conn_pool: pool,
            inner: Arc::new(tokio::sync::RwLock::new(RabbitMQInner {
                publish_channel,
                consume_channel,
            })),
            can_reconnect: Arc::new(AtomicBool::new(true)),
            max_reconnection_attempts: 200,
            max_reconnection_delay: Duration::from_secs(1),
            nack_on_failure: opt.nack_on_failure,
            requeue_on_failure: opt.requeue_on_failure,
            execute_handler_asynchronously: opt.execute_handler_asynchronously,
            default_publish_options: BasicPublishOptions::default(),
            default_publish_props: BasicProperties::default(),
            default_consume_options: BasicConsumeOptions::default(),
            operation_timeout: Duration::from_secs(30),
            health_check_interval: 100,
            prefetch_count: 10,
            cancellation_token: CancellationToken::new(),
        })
    }

    /// Configure nack on failure (default: true)
    pub fn nack_on_failure(&mut self, state: bool) -> &mut Self {
        self.nack_on_failure = state;
        self
    }

    /// Configure requeue on failure (default: true)
    pub fn requeue_on_failure(&mut self, state: bool) -> &mut Self {
        self.requeue_on_failure = state;
        self
    }

    /// Configure async handler execution (default: true)
    pub fn execute_handler_asynchronously(&mut self, state: bool) -> &mut Self {
        self.execute_handler_asynchronously = state;
        self
    }

    /// Set operation timeout (default: 30s)
    pub fn operation_timeout(&mut self, timeout: Duration) -> &mut Self {
        self.operation_timeout = timeout;
        self
    }

    /// Set health check interval (default: 100 messages, 0 to disable)
    pub fn health_check_interval(&mut self, interval: usize) -> &mut Self {
        self.health_check_interval = interval;
        self
    }

    /// Set QoS prefetch count (default: 10, recommended: 10-100)
    pub fn prefetch_count(&mut self, count: u16) -> &mut Self {
        self.prefetch_count = count;
        self
    }

    /// Set the maximum number of reconnection attempts.
    ///
    /// Default: 200. With exponential backoff (1s → 60s cap), this provides
    /// approximately 3–4 hours of retry before giving up.
    pub fn max_reconnection_attempts(&mut self, attempts: usize) -> &mut Self {
        self.max_reconnection_attempts = attempts;
        self
    }

    /// Get cancellation token for external shutdown control
    pub fn get_cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }

    /// Gracefully shut down all consumers
    pub fn shutdown(&self) {
        info!("RabbitMQ shutdown requested");
        self.cancellation_token.cancel();
    }

    /// Create a message publisher builder for flexible message publishing.
    ///
    /// This provides a composable way to publish messages with various options
    /// like custom properties, delays, headers, etc.
    ///
    /// # Example
    /// ```rust,no_run
    /// use foxtive::prelude::RabbitMQ;
    /// use std::time::Duration;
    ///
    /// # async fn example(pool: deadpool_lapin::Pool) {
    /// let rmq = RabbitMQ::new(pool).await.unwrap();
    ///
    /// // Simple publish
    /// rmq.publisher()
    ///     .exchange("events")
    ///     .routing_key("user.created")
    ///     .payload(b"{\"user_id\": 123}")
    ///     .send().await.unwrap();
    ///
    /// // Publish with delay and custom headers
    /// rmq.publisher()
    ///     .exchange("delayed-events")
    ///     .routing_key("user.reminder")
    ///     .payload(b"{\"reminder\": true}")
    ///     .delay(Duration::from_secs(300))
    ///     .send().await.unwrap();
    /// # }
    /// ```
    pub fn publisher(&self) -> MessagePublisher<'_> {
        MessagePublisher::new(self)
    }

    /// Set the setup function directly (used by builder wiring).
    pub fn setup_fn_raw(&mut self, func: RabbitMQSetupFn) -> &mut Self {
        self.setup_fn = Some(func);
        self
    }

    /// Setup function to run after the connection is established.
    pub async fn setup_fn<F>(&mut self, func: F) -> &mut Self
    where
        F: Fn(Self) -> BoxFuture<'static, RmqResult<()>> + Send + Sync + 'static,
    {
        info!("Running setup function...");
        match func(self.clone()).await {
            Ok(_) => info!("Setup function completed successfully."),
            Err(err) => error!("Setup function failed: {err}"),
        };

        self.setup_fn = Some(Arc::new(func));

        self
    }

    pub async fn declare_exchange(
        &self,
        exchange: &str,
        kind: ExchangeKind,
        options: ExchangeDeclareOptions,
        args: FieldTable,
    ) -> RmqResult<()> {
        self.ensure_channel_is_usable(true).await?;

        let inner = self.inner.read().await;
        tokio::time::timeout(
            self.operation_timeout,
            inner
                .publish_channel
                .exchange_declare(exchange.into(), kind, options, args),
        )
        .await
        .map_err(|_| RmqError::timeout("exchange_declare", self.operation_timeout))??;

        Ok(())
    }

    pub async fn declare_queue(
        &self,
        queue: &str,
        options: QueueDeclareOptions,
        args: FieldTable,
    ) -> RmqResult<()> {
        self.ensure_channel_is_usable(true).await?;

        let inner = self.inner.read().await;
        tokio::time::timeout(
            self.operation_timeout,
            inner.publish_channel.queue_declare(queue.into(), options, args),
        )
        .await
        .map_err(|_| RmqError::timeout("queue_declare", self.operation_timeout))??;

        Ok(())
    }

    pub async fn bind_queue<R: ToString>(
        &self,
        queue: &str,
        exchange: &str,
        routing_key: R,
        options: QueueBindOptions,
        args: FieldTable,
    ) -> RmqResult<()> {
        self.ensure_channel_is_usable(true).await?;

        let inner = self.inner.read().await;
        tokio::time::timeout(
            self.operation_timeout,
            inner.publish_channel.queue_bind(
                queue.into(),
                exchange.into(),
                routing_key.to_string().into(),
                options,
                args,
            ),
        )
        .await
        .map_err(|_| RmqError::timeout("queue_bind", self.operation_timeout))??;

        Ok(())
    }

    pub async fn publish<E, R>(&self, exchange: E, routing_key: R, payload: &[u8]) -> RmqResult<()>
    where
        E: ToString,
        R: ToString,
    {
        // Use the new builder internally for consistency
        self.publisher()
            .exchange(exchange)
            .routing_key(routing_key)
            .payload(payload)
            .send()
            .await
    }

    pub async fn consume<F, Fut>(&self, queue: &str, tag: &str, func: F) -> RmqResult<()>
    where
        F: Fn(Message) -> Fut + Send + Clone + 'static,
        Fut: Future<Output = RmqResult<()>> + Send + 'static,
    {
        info!("Subscribing to '{queue}'...");
        let mut retry_delay = Self::RETRY_DELAY;
        const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);

        loop {
            // Check cancellation before each retry
            if self.cancellation_token.is_cancelled() {
                info!("[{tag}] Cancellation requested, stopping consumer");
                return Ok(());
            }

            match self.start_consume(queue, tag, func.clone()).await {
                Ok(_) => {
                    info!("[{tag}] Consumer stopped normally");
                    break;
                }
                Err(err) => {
                    error!(
                        "[{tag}] Consumer encountered an error: {err:?}, retrying in {retry_delay:?}..."
                    );
                    sleep(retry_delay).await;
                    retry_delay = std::cmp::min(retry_delay.saturating_mul(2), MAX_RETRY_DELAY);
                }
            }
        }
        Ok(())
    }

    /// Consume a queue forever, restarting if it fails.
    /// Respects the cancellation token for graceful shutdown.
    pub async fn consume_forever<F, Fut>(&self, queue: &str, tag: &str, func: F) -> RmqResult<()>
    where
        F: Fn(Message) -> Fut + Send + Clone + 'static,
        Fut: Future<Output = RmqResult<()>> + Send + 'static,
    {
        loop {
            if self.cancellation_token.is_cancelled() {
                info!("[{tag}] Cancellation requested, stopping consume_forever");
                return Ok(());
            }

            match self.consume(queue, tag, func.clone()).await {
                Ok(_) => {
                    warn!("[{tag}] Consumer stopped unexpectedly, restarting...");
                }
                Err(err) => {
                    error!("[{tag}] Consumer encountered an error: {err:?}, retrying...");
                }
            }

            // Check cancellation before sleeping
            tokio::select! {
                _ = self.cancellation_token.cancelled() => {
                    info!("[{tag}] Cancellation requested during backoff, stopping");
                    return Ok(());
                }
                _ = sleep(Self::RETRY_DELAY) => {}
            }
        }
    }

    async fn start_consume<F, Fut>(&self, queue: &str, tag: &str, func: F) -> RmqResult<()>
    where
        F: Fn(Message) -> Fut + Send + Clone + 'static,
        Fut: Future<Output = RmqResult<()>> + Send + 'static,
    {
        self.ensure_channel_is_usable(false).await?;

        // Acquire read lock just long enough to start the consumer.
        let mut consumer = {
            let inner = self.inner.read().await;
            tokio::time::timeout(
                self.operation_timeout,
                inner.consume_channel.basic_consume(
                    queue.into(),
                    tag.into(),
                    self.default_consume_options,
                    FieldTable::default(),
                ),
            )
            .await
            .map_err(|_| RmqError::timeout("basic_consume", self.operation_timeout))??
        };

        // Configure QoS prefetch
        if self.prefetch_count > 0 {
            let inner = self.inner.read().await;
            tokio::time::timeout(
                self.operation_timeout,
                inner
                    .consume_channel
                    .basic_qos(self.prefetch_count, BasicQosOptions { global: false }),
            )
            .await
            .map_err(|_| RmqError::timeout("basic_qos", self.operation_timeout))?
            .inspect_err(|e| warn!("Failed to set QoS prefetch: {e:?}"))
            .ok();

            debug!("[{tag}] QoS prefetch set to {}", self.prefetch_count);
        }

        let instance = self.clone();
        let cancellation_token = self.cancellation_token.clone();
        let mut message_count: usize = 0;

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    info!("[{tag}] Cancellation requested, shutting down consumer gracefully");
                    return Ok(());
                }

                result = consumer.next() => {
                    match result {
                        Some(Ok(delivery)) => {
                            // Periodic health check
                            if self.health_check_interval > 0 {
                                message_count += 1;
                                if message_count.is_multiple_of(self.health_check_interval) {
                                    debug!("[{tag}] Health check: processed {message_count} messages");

                                    if let Err(err) = self.conn_pool.get().await {
                                        warn!("[{tag}] Health check failed - connection pool error: {err:?}");
                                        return Err(RmqError::health_check_failed("connection pool unavailable"));
                                    }

                                    let inner = self.inner.read().await;
                                    let channel_status = inner.consume_channel.status().clone();
                                    drop(inner);
                                    if !channel_status.connected() || channel_status.closing() {
                                        warn!("[{tag}] Health check failed - channel status: {channel_status:?}");
                                        return Err(RmqError::channel_error(format!("{:?}", channel_status), 0));
                                    }
                                }
                            }

                            let instance = instance.clone();
                    let consumer_tag = tag.to_owned();

                    let func_clone = func.clone();
                    let handler = async move {
                        let delivery_tag = delivery.delivery_tag;
                        match func_clone(Message::new(delivery)).await {
                            Ok(_) => {}, // User handles ack
                            Err(err) => {
                                error!("[consume-executor][{consumer_tag}] Handler returned error: {err:?}");
                                if instance.nack_on_failure
                                    && let Err(nack_err) = instance.nack(delivery_tag, instance.requeue_on_failure).await
                                {
                                    error!("[consume-executor][{consumer_tag}] Failed to nack message {delivery_tag}: {nack_err:?}");
                                }
                            }
                        }
                    };

                    if self.execute_handler_asynchronously {
                        Handle::current().spawn(async move {
                            handler.await;
                        });
                    } else {
                        handler.await;
                    }
                        }
                        Some(Err(err)) => {
                            error!("[{tag}] Consumer stream error: {err:?}");
                            return Err(err.into());
                        }
                        None => {
                            error!("[{tag}] Consumer stream ended unexpectedly");
                            return Err(RmqError::stream_terminated(queue, tag));
                        }
                    }
                }
            } // End of tokio::select!
        } // End of loop
    }

    /// Consume messages from a specified queue and execute an async function on each message
    /// This method will run in detached mode :)
    pub async fn consume_detached<F, Fut>(
        &self,
        queue: &str,
        tag: &str,
        func: F,
    ) -> JoinHandle<RmqResult<()>>
    where
        F: Fn(Message) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = RmqResult<()>> + Send + 'static,
    {
        let tag = tag.to_owned();
        let queue = queue.to_owned();
        let instance = self.clone();
        Handle::current().spawn(async move { instance.consume(&queue, &tag, func).await })
    }

    /// Consume a queue forever, restarting if it fails.
    /// This method will run in detached mode :)
    pub async fn consume_forever_detached<F, Fut>(
        &self,
        queue: &str,
        tag: &str,
        func: F,
    ) -> JoinHandle<RmqResult<()>>
    where
        F: Fn(Message) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = RmqResult<()>> + Send + 'static,
    {
        let tag = tag.to_owned();
        let queue = queue.to_owned();
        let instance = self.clone();
        Handle::current().spawn(async move { instance.consume_forever(&queue, &tag, func).await })
    }

    // ==================== Pull-Based Consumer Methods ====================

    /// Create a pull-based consumer that returns messages via an async stream.
    ///
    /// This method provides a more flexible alternative to callback-based consumption,
    /// allowing you to receive messages on-demand using `.next().await`.
    ///
    /// # Returns
    /// A tuple of `(ConsumerStream, ConsumerGuard)` where:
    /// - `ConsumerStream` implements `Stream<Item = RmqResult<Message>>`
    /// - `ConsumerGuard` must be kept alive; dropping it cancels the consumer
    ///
    /// # Example
    /// ```rust,no_run
    /// use foxtive::prelude::RabbitMQ;
    /// use futures_util::StreamExt;
    ///
    /// # async fn example(pool: deadpool_lapin::Pool) {
    /// let rmq = RabbitMQ::new(pool).await.unwrap();
    ///
    /// let (mut stream, _guard) = rmq.create_consumer("my_queue", "my-consumer").await.unwrap();
    ///
    /// while let Some(message_result) = stream.next().await {
    ///     match message_result {
    ///         Ok(message) => {
    ///             println!("Received: {:?}", message.data());
    ///             message.ack().await.unwrap();
    ///         }
    ///         Err(e) => eprintln!("Error: {}", e),
    ///     }
    /// }
    /// # }
    /// ```
    pub async fn create_consumer(
        &self,
        queue: &str,
        tag: &str,
    ) -> RmqResult<(ConsumerStream, ConsumerGuard)> {
        self.ensure_channel_is_usable(false).await?;

        // Configure QoS prefetch
        if self.prefetch_count > 0 {
            let inner = self.inner.read().await;
            tokio::time::timeout(
                self.operation_timeout,
                inner
                    .consume_channel
                    .basic_qos(self.prefetch_count, BasicQosOptions { global: false }),
            )
            .await
            .map_err(|_| RmqError::timeout("basic_qos", self.operation_timeout))?
            .inspect_err(|e| warn!("Failed to set QoS prefetch: {e:?}"))
            .ok();
        }

        // Start consumer (acquire read lock just long enough)
        let consumer = {
            let inner = self.inner.read().await;
            tokio::time::timeout(
                self.operation_timeout,
                inner.consume_channel.basic_consume(
                    queue.into(),
                    tag.into(),
                    self.default_consume_options,
                    FieldTable::default(),
                ),
            )
            .await
            .map_err(|_| RmqError::timeout("basic_consume", self.operation_timeout))??
        };

        debug!(
            "Created pull-based consumer for queue '{}' with tag '{}'",
            queue, tag
        );

        // Create channel for message delivery
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let cancellation_token = self.cancellation_token.clone();

        // Spawn task to forward deliveries to channel
        let consumer_tag = tag.to_string();
        let forwarder_handle = Handle::current().spawn(async move {
            let mut consumer = consumer;
            loop {
                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        debug!("[{}] Consumer cancelled", consumer_tag);
                        break;
                    }
                    delivery = consumer.next() => {
                        match delivery {
                            Some(Ok(delivery)) => {
                                if tx.send(Ok(Message::new(delivery))).await.is_err() {
                                    debug!("[{}] Receiver dropped, stopping forwarder", consumer_tag);
                                    break;
                                }
                            }
                            Some(Err(e)) => {
                                error!("[{}] Consumer error: {:?}", consumer_tag, e);
                                if tx.send(Err(e.into())).await.is_err() {
                                    break;
                                }
                            }
                            None => {
                                warn!("[{}] Consumer stream ended", consumer_tag);
                                break;
                            }
                        }
                    }
                }
            }
        });

        let stream = ConsumerStream {
            inner: tokio_stream::wrappers::ReceiverStream::new(rx),
        };

        let guard = ConsumerGuard {
            handle: Some(forwarder_handle),
            cancellation_token: self.cancellation_token.clone(),
        };

        Ok((stream, guard))
    }

    /// Receive a single message from a queue (blocking until available or timeout).
    ///
    /// This is a convenience method for simple pull-based consumption without
    /// managing streams or guards.
    ///
    /// # Arguments
    /// * `queue` - Queue name to consume from
    /// * `tag` - Consumer tag (identifier)
    /// * `timeout` - Optional timeout duration (None = wait indefinitely)
    ///
    /// # Returns
    /// * `Ok(Some(message))` - Message received
    /// * `Ok(None)` - Timeout elapsed or consumer cancelled
    /// * `Err(RmqError)` - Error occurred
    ///
    /// # Example
    /// ```rust,no_run
    /// use foxtive::prelude::RabbitMQ;
    /// use std::time::Duration;
    ///
    /// # async fn example(pool: deadpool_lapin::Pool) {
    /// let rmq = RabbitMQ::new(pool).await.unwrap();
    ///
    /// // Wait up to 5 seconds for a message
    /// if let Ok(Some(message)) = rmq.receive_message("my_queue", "worker-1", Some(Duration::from_secs(5))).await {
    ///     println!("Got message!");
    ///     message.ack().await.unwrap();
    /// }
    /// # }
    /// ```
    pub async fn receive_message(
        &self,
        queue: &str,
        tag: &str,
        timeout: Option<Duration>,
    ) -> RmqResult<Option<Message>> {
        let (mut stream, _guard) = self.create_consumer(queue, tag).await?;

        match timeout {
            Some(duration) => {
                match tokio::time::timeout(duration, stream.next()).await {
                    Ok(Some(result)) => result.map(Some),
                    Ok(None) => Ok(None), // Stream ended
                    Err(_) => {
                        debug!("Receive timed out after {:?}", duration);
                        Ok(None)
                    }
                }
            }
            None => {
                // Wait indefinitely
                match stream.next().await {
                    Some(result) => result.map(Some),
                    None => Ok(None), // Stream ended
                }
            }
        }
    }

    pub async fn ack(&self, delivery_tag: u64) -> RmqResult<()> {
        self.ensure_channel_is_usable(false).await?;

        let inner = self.inner.read().await;
        tokio::time::timeout(
            self.operation_timeout,
            inner
                .consume_channel
                .basic_ack(delivery_tag, BasicAckOptions::default()),
        )
        .await
        .map_err(|_| RmqError::timeout("basic_ack", self.operation_timeout))??;

        Ok(())
    }

    pub async fn nack(&self, delivery_tag: u64, requeue: bool) -> RmqResult<()> {
        self.ensure_channel_is_usable(false).await?;

        let inner = self.inner.read().await;
        tokio::time::timeout(
            self.operation_timeout,
            inner.consume_channel.basic_nack(
                delivery_tag,
                BasicNackOptions {
                    multiple: false,
                    requeue,
                },
            ),
        )
        .await
        .map_err(|_| RmqError::timeout("basic_nack", self.operation_timeout))??;

        Ok(())
    }

    /// Request a connection close.
    ///
    /// This method is only successful if the connection is in the connected state,
    /// otherwise an [`InvalidConnectionState`] error is returned.
    ///
    pub async fn close(&self, reply_code: ReplyCode, reply_text: &str) -> RmqResult<()> {
        let connection = self.conn_pool.get().await?;
        self.can_reconnect.store(false, Ordering::SeqCst);
        Ok(connection.close(reply_code, reply_text.into()).await?)
    }

    /// Acquire connection pool in use by this instance
    pub fn connection_pool(&self) -> deadpool_lapin::Pool {
        self.conn_pool.clone()
    }

    pub async fn close_channels(&self, reply_code: ReplyCode, reply_text: &str) -> RmqResult<()> {
        let inner = self.inner.read().await;
        inner.publish_channel.close(reply_code, reply_text.into()).await?;
        inner.consume_channel.close(reply_code, reply_text.into()).await?;
        Ok(())
    }

    /// Check if setup function is set
    pub fn has_setup_fn(&self) -> bool {
        self.setup_fn.is_some()
    }

    async fn ensure_channel_is_usable(&self, is_publish_channel: bool) -> RmqResult<()> {
        loop {
            // Check connection pool first (no lock needed)
            let connection = self.conn_pool.get().await;
            if connection.is_err() {
                warn!("Lost connection to RabbitMQ, attempting to reconnect...");
                self.recreate_connection().await?;
                continue;
            }

            // Check channel state under read lock
            let needs_recreate = {
                let inner = self.inner.read().await;
                let channel = if is_publish_channel {
                    &inner.publish_channel
                } else {
                    &inner.consume_channel
                };
                let status = channel.status();
                if !status.connected() || status.closing() {
                    warn!(
                        "Channel({}) is not usable: {status:?}, recreating...",
                        channel.id()
                    );
                    true
                } else {
                    false
                }
            };

            if needs_recreate {
                self.recreate_channel(is_publish_channel).await?;
            } else {
                break;
            }
        }

        Ok(())
    }

    /// Execute setup function if configured
    pub(crate) async fn setup(&self) -> RmqResult<()> {
        match &self.setup_fn {
            Some(func) => {
                info!("Executing user-defined setup function...");
                func(self.clone()).await?;
                info!("Setup function executed successfully.");
            }
            None => {
                warn!("No setup function provided, skipping...");
            }
        }

        Ok(())
    }

    async fn recreate_channel(&self, is_publish_channel: bool) -> RmqResult<()> {
        info!("Recreating unusable channel...");

        if !self.can_reconnect.load(Ordering::SeqCst) {
            warn!("Cannot reconnect, channel recreation aborted");
            return Err(lapin::Error::from(lapin::ErrorKind::InvalidConnectionState(
                ConnectionState::Closed,
            ))
            .into());
        }

        let connection = self.conn_pool.get().await?;

        if !connection.status().connected() {
            warn!("Connection is not usable: {:?}, attempting to re-establish...", connection.status());
            self.recreate_connection().await?;
        }

        info!("Performing channel recreation...");
        let result = connection.create_channel().await;

        if result.is_err() {
            warn!("Failed to recreate channel, attempting to re-establish connection...");
            self.recreate_connection().await?;
        }

        let new_channel = connection.create_channel().await?;
        let channel_id = new_channel.id();

        // Acquire write lock to swap the channel
        {
            let mut inner = self.inner.write().await;
            if is_publish_channel {
                inner.publish_channel = new_channel;
            } else {
                inner.consume_channel = new_channel;
            }
        }

        info!("Channel({channel_id}) recreated");

        self.setup().await?;

        sleep(Duration::from_secs(1)).await;

        Ok(())
    }

    async fn recreate_connection(&self) -> RmqResult<()> {
        if !self.can_reconnect.load(Ordering::SeqCst) {
            warn!("Cannot reconnect, re-establishing connection aborted");
            return Err(lapin::Error::from(lapin::ErrorKind::InvalidConnectionState(
                ConnectionState::Closed,
            ))
            .into());
        }

        const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(60);
        let mut delay = self.max_reconnection_delay;
        for attempt in 1..=self.max_reconnection_attempts {
            info!("Attempting to reconnect to RabbitMQ, attempt {attempt}...");
            match self.conn_pool.get().await {
                Ok(_) => {
                    info!("Reconnected to RabbitMQ successfully on attempt {attempt}");
                    return Ok(());
                }
                Err(err) => {
                    warn!("Failed to reconnect to RabbitMQ (attempt {attempt}): {err}");
                    sleep(delay).await;
                    delay = std::cmp::min(delay.saturating_mul(2), MAX_RECONNECT_DELAY);
                }
            }
        }

        error!("Max reconnection attempts reached, giving up");
        Err(lapin::Error::from(lapin::ErrorKind::InvalidConnectionState(
            ConnectionState::Closed,
        ))
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_rabbitmq_options_default() {
        let opts = RabbitMQOptions::default();
        assert!(!opts.nack_on_failure);
        assert!(!opts.requeue_on_failure);
        assert!(!opts.execute_handler_asynchronously);
    }

    #[test]
    fn test_rabbitmq_options_custom() {
        let opts = RabbitMQOptions {
            nack_on_failure: false,
            requeue_on_failure: false,
            execute_handler_asynchronously: false,
        };
        assert!(!opts.nack_on_failure);
        assert!(!opts.requeue_on_failure);
        assert!(!opts.execute_handler_asynchronously);
    }

    #[tokio::test]
    async fn test_operation_timeout_configuration() {
        let config = deadpool_lapin::Config {
            url: Some("amqp://localhost:5672".to_string()),
            ..Default::default()
        };
        let pool = config
            .create_pool(
                lapin::ConnectionProperties::default,
                deadpool_lapin::Runtime::Tokio1,
            )
            .unwrap();

        // Expect failure without running server
        let result = RabbitMQ::new(pool).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_error_type_variants_exist() {
        let _err1 = RmqError::Generic("test".to_string());
        let _err2 = RmqError::ShutdownRequested;
        let _err3 = RmqError::timeout("op", Duration::from_secs(1));
        let _err4 = RmqError::stream_terminated("q", "t");
        let _err5 = RmqError::health_check_failed("reason");
        let _err6 = RmqError::channel_error("state", 1);
        let _err7 = RmqError::Configuration {
            message: "msg".to_string(),
        };
        let _err8 = RmqError::ReconnectionFailed { attempts: 1 };
    }

    #[test]
    fn test_result_type_alias() {
        let success: RmqResult<()> = Ok(());
        let failure: RmqResult<()> = Err(RmqError::Generic("error".to_string()));

        assert!(success.is_ok());
        assert!(failure.is_err());
    }
}
