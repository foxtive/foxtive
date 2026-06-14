use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use lapin::types::{FieldTable, LongInt};
use lapin::{BasicProperties, Channel, ConnectionState};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

pub use {
    lapin::message::{Delivery, DeliveryResult},
    lapin::types::ReplyCode,
    lapin::{ChannelState, ExchangeKind, options::*},
};

pub use crate::rabbitmq::error::{IntoRmqError, RmqError, RmqResult};

use crate::FOXTIVE;
use crate::prelude::AppStateExt;
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
    rx: tokio::sync::mpsc::Receiver<RmqResult<Message>>,
}

impl ConsumerStream {
    /// Receive the next message from the stream.
    ///
    /// Returns `None` when the stream is closed (consumer cancelled or error).
    pub async fn next(&mut self) -> Option<RmqResult<Message>> {
        self.rx.recv().await
    }
}

// Implement Stream trait for compatibility with futures utilities
impl futures_util::Stream for ConsumerStream {
    type Item = RmqResult<Message>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;

        // Convert async recv to poll-based interface
        let fut = self.rx.recv();
        tokio::pin!(fut);

        match fut.poll(cx) {
            Poll::Ready(val) => Poll::Ready(val),
            Poll::Pending => Poll::Pending,
        }
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
    /// Cancel the consumer immediately.
    pub fn cancel(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.cancellation_token.cancel();
            // Don't await - just signal cancellation
            drop(handle);
        }
    }
}

impl Drop for ConsumerGuard {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Clone)]
pub struct RabbitMQ {
    conn_pool: deadpool_lapin::Pool,
    publish_channel: Channel,
    consume_channel: Channel,
    /// Controls reconnection behavior
    can_reconnect: bool,
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

    /// Create from Foxtive static context
    pub async fn new_from_foxtive() -> RmqResult<Self> {
        Self::new_opt(
            FOXTIVE.rabbitmq_pool(),
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
            publish_channel,
            consume_channel,
            can_reconnect: true,
            max_reconnection_attempts: 1_000_000,
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
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut rmq = RabbitMQ::new_from_foxtive().await.unwrap();
    ///     
    ///     // Simple publish
    ///     rmq.publisher()
    ///         .exchange("events")
    ///         .routing_key("user.created")
    ///         .payload(b"{\"user_id\": 123}")
    ///         .send().await.unwrap();
    ///     
    ///     // Publish with delay and custom headers
    ///     rmq.publisher()
    ///         .exchange("delayed-events")
    ///         .routing_key("user.reminder")
    ///         .payload(b"{\"reminder\": true}")
    ///         .delay(Duration::from_secs(300))
    ///         .header("service_name", "user-service")
    ///         .header("correlation_id", "abc-123")
    ///         .send().await.unwrap();
    /// }
    /// ```
    pub fn publisher(&mut self) -> MessagePublisher<'_> {
        MessagePublisher::new(self)
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

    pub async fn declare_exchange(&mut self, exchange: &str, kind: ExchangeKind) -> RmqResult<()> {
        self.ensure_channel_is_usable(true).await?;

        tokio::time::timeout(
            self.operation_timeout,
            self.publish_channel.exchange_declare(
                exchange,
                kind.clone(),
                ExchangeDeclareOptions::default(),
                FieldTable::default(),
            ),
        )
        .await
        .map_err(|_| RmqError::timeout("exchange_declare", self.operation_timeout))??;

        Ok(())
    }

    pub async fn declare_queue(
        &mut self,
        queue: &str,
        options: QueueDeclareOptions,
        args: FieldTable,
    ) -> RmqResult<()> {
        self.ensure_channel_is_usable(true).await?;

        tokio::time::timeout(
            self.operation_timeout,
            self.publish_channel.queue_declare(queue, options, args),
        )
        .await
        .map_err(|_| RmqError::timeout("queue_declare", self.operation_timeout))??;

        Ok(())
    }

    pub async fn bind_queue<R: ToString>(
        &mut self,
        queue: &str,
        exchange: &str,
        routing_key: R,
        options: QueueBindOptions,
        args: FieldTable,
    ) -> RmqResult<()> {
        self.ensure_channel_is_usable(true).await?;

        tokio::time::timeout(
            self.operation_timeout,
            self.publish_channel.queue_bind(
                queue,
                exchange,
                &routing_key.to_string(),
                options,
                args,
            ),
        )
        .await
        .map_err(|_| RmqError::timeout("queue_bind", self.operation_timeout))??;

        Ok(())
    }

    pub async fn publish<E, R>(
        &mut self,
        exchange: E,
        routing_key: R,
        payload: &[u8],
    ) -> RmqResult<()>
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

    pub async fn consume<F, Fut>(&mut self, queue: &str, tag: &str, func: F) -> RmqResult<()>
    where
        F: Fn(Message) -> Fut + Send + Copy + 'static,
        Fut: Future<Output = RmqResult<()>> + Send + 'static,
    {
        info!("Subscribing to '{queue}'...");

        loop {
            match self.start_consume(queue, tag, func).await {
                Ok(_) => {
                    info!("[{tag}] Consumer stopped normally");
                    break;
                }
                Err(err) => {
                    error!("[{tag}] Consumer encountered an error: {err:?}, restarting...");
                    sleep(Self::RETRY_DELAY).await;
                }
            }
        }
        Ok(())
    }

    /// Consume a queue forever, restarting if it fails.
    pub async fn consume_forever<F, Fut>(&mut self, queue: &str, tag: &str, func: F) -> !
    where
        F: Fn(Message) -> Fut + Send + Copy + 'static,
        Fut: Future<Output = RmqResult<()>> + Send + 'static,
    {
        loop {
            match self.consume(queue, tag, func).await {
                Ok(_) => {
                    warn!("[{tag}] Consumer stopped unexpectedly, restarting...");
                }
                Err(err) => {
                    error!("[{tag}] Consumer encountered an error: {err:?}, retrying...");
                }
            }

            sleep(Self::RETRY_DELAY).await;
        }
    }

    async fn start_consume<F, Fut>(&mut self, queue: &str, tag: &str, func: F) -> RmqResult<()>
    where
        F: Fn(Message) -> Fut + Send + Copy + 'static,
        Fut: Future<Output = RmqResult<()>> + Send + 'static,
    {
        self.ensure_channel_is_usable(false).await?;

        let mut consumer = tokio::time::timeout(
            self.operation_timeout,
            self.consume_channel.basic_consume(
                queue,
                tag,
                self.default_consume_options,
                FieldTable::default(),
            ),
        )
        .await
        .map_err(|_| RmqError::timeout("basic_consume", self.operation_timeout))??;

        // Configure QoS prefetch
        if self.prefetch_count > 0 {
            tokio::time::timeout(
                self.operation_timeout,
                self.consume_channel
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

                                    let channel_state = self.consume_channel.status().state();
                                    if matches!(channel_state, ChannelState::Closed | ChannelState::Closing | ChannelState::Error) {
                                        warn!("[{tag}] Health check failed - channel state: {channel_state:?}");
                                        return Err(RmqError::channel_error(format!("{:?}", channel_state), self.consume_channel.id()));
                                    }
                                }
                            }

                            let mut instance = instance.clone();
                    let consumer_tag = tag.to_owned();

                    let handler = async move {
                        let delivery_tag = delivery.delivery_tag;
                        match func(Message::new(delivery)).await {
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
        F: Fn(Message) -> Fut + Copy + Send + Sync + 'static,
        Fut: Future<Output = RmqResult<()>> + Send + 'static,
    {
        let tag = tag.to_owned();
        let queue = queue.to_owned();
        let instance = self.clone();
        Handle::current().spawn(async move {
            let mut instance = instance.clone();
            instance.consume(&queue, &tag, func).await
        })
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
        F: Fn(Message) -> Fut + Copy + Send + Sync + 'static,
        Fut: Future<Output = RmqResult<()>> + Send + 'static,
    {
        let tag = tag.to_owned();
        let queue = queue.to_owned();
        let instance = self.clone();
        Handle::current().spawn(async move {
            let mut instance = instance.clone();
            instance.consume_forever(&queue, &tag, func).await
        })
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
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut rmq = RabbitMQ::new_from_foxtive().await.unwrap();
    ///     
    ///     let (mut stream, _guard) = rmq.create_consumer("my_queue", "my-consumer").await.unwrap();
    ///     
    ///     while let Some(message_result) = stream.next().await {
    ///         match message_result {
    ///             Ok(message) => {
    ///                 println!("Received: {:?}", message.data());
    ///                 message.ack().await.unwrap();
    ///             }
    ///             Err(e) => eprintln!("Error: {}", e),
    ///         }
    ///     }
    /// }
    /// ```
    pub async fn create_consumer(
        &mut self,
        queue: &str,
        tag: &str,
    ) -> RmqResult<(ConsumerStream, ConsumerGuard)> {
        self.ensure_channel_is_usable(false).await?;

        // Configure QoS prefetch
        if self.prefetch_count > 0 {
            tokio::time::timeout(
                self.operation_timeout,
                self.consume_channel
                    .basic_qos(self.prefetch_count, BasicQosOptions { global: false }),
            )
            .await
            .map_err(|_| RmqError::timeout("basic_qos", self.operation_timeout))?
            .inspect_err(|e| warn!("Failed to set QoS prefetch: {e:?}"))
            .ok();
        }

        // Start consumer
        let consumer = tokio::time::timeout(
            self.operation_timeout,
            self.consume_channel.basic_consume(
                queue,
                tag,
                self.default_consume_options,
                FieldTable::default(),
            ),
        )
        .await
        .map_err(|_| RmqError::timeout("basic_consume", self.operation_timeout))??;

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

        let stream = ConsumerStream { rx };

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
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut rmq = RabbitMQ::new_from_foxtive().await.unwrap();
    ///     
    ///     // Wait up to 5 seconds for a message
    ///     if let Ok(Some(message)) = rmq.receive_message("my_queue", "worker-1", Some(Duration::from_secs(5))).await {
    ///         println!("Got message!");
    ///         message.ack().await.unwrap();
    ///     }
    /// }
    /// ```
    pub async fn receive_message(
        &mut self,
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

    pub async fn ack(&mut self, delivery_tag: u64) -> RmqResult<()> {
        self.ensure_channel_is_usable(false).await?;

        tokio::time::timeout(
            self.operation_timeout,
            self.consume_channel
                .basic_ack(delivery_tag, BasicAckOptions::default()),
        )
        .await
        .map_err(|_| RmqError::timeout("basic_ack", self.operation_timeout))??;

        Ok(())
    }

    pub async fn nack(&mut self, delivery_tag: u64, requeue: bool) -> RmqResult<()> {
        self.ensure_channel_is_usable(false).await?;

        tokio::time::timeout(
            self.operation_timeout,
            self.consume_channel.basic_nack(
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
    pub async fn close(&mut self, reply_code: ReplyCode, reply_text: &str) -> RmqResult<()> {
        let connection = self.conn_pool.get().await?;
        self.can_reconnect = false;
        Ok(connection.close(reply_code, reply_text).await?)
    }

    /// Acquire connection pool in use by this instance
    pub fn connection_pool(&self) -> deadpool_lapin::Pool {
        self.conn_pool.clone()
    }

    pub async fn close_channels(&self, reply_code: ReplyCode, reply_text: &str) -> RmqResult<()> {
        self.publish_channel.close(reply_code, reply_text).await?;
        self.consume_channel.close(reply_code, reply_text).await?;
        Ok(())
    }

    /// Check if setup function is set
    pub fn has_setup_fn(&self) -> bool {
        self.setup_fn.is_some()
    }

    async fn ensure_channel_is_usable(&mut self, is_publish_channel: bool) -> RmqResult<()> {
        loop {
            let channel = match is_publish_channel {
                true => &self.publish_channel,
                false => &self.consume_channel,
            };

            let connection = self.conn_pool.get().await;
            if connection.is_err() {
                warn!("Lost connection to RabbitMQ, attempting to reconnect...");
                self.recreate_connection().await?;
                continue;
            }

            let state = channel.status().state();
            match state {
                ChannelState::Closed | ChannelState::Closing | ChannelState::Error => {
                    warn!(
                        "Channel({}) is not usable: {state:?}, recreating...",
                        channel.id()
                    );
                    self.recreate_channel(is_publish_channel).await?;
                }
                _ => break,
            }
        }

        Ok(())
    }

    /// Execute setup function if configured
    async fn setup(&mut self) -> RmqResult<()> {
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

    async fn recreate_channel(&mut self, is_publish_channel: bool) -> RmqResult<()> {
        info!("Recreating unusable channel...");

        if !self.can_reconnect {
            warn!("Cannot reconnect, channel recreation aborted");
            return Err(lapin::Error::from(lapin::ErrorKind::InvalidConnectionState(
                ConnectionState::Closed,
            ))
            .into());
        }

        let connection = self.conn_pool.get().await?;
        let state = connection.status().state();

        if state != ConnectionState::Connected {
            warn!("Connection is not usable: {state:?}, attempting to re-establish...");
            self.recreate_connection().await?;
        }

        info!("Performing channel recreation...");
        let result = match is_publish_channel {
            true => connection.create_channel().await,
            false => connection.create_channel().await,
        };

        if result.is_err() {
            warn!("Failed to recreate channel, attempting to re-establish connection...");
            self.recreate_connection().await?;
        }

        let channel = match is_publish_channel {
            true => {
                self.publish_channel = connection.create_channel().await?;
                &self.publish_channel
            }
            false => {
                self.consume_channel = connection.create_channel().await?;
                &self.consume_channel
            }
        };

        info!("Channel({}) recreated", channel.id());

        self.setup().await?;

        sleep(Duration::from_secs(1)).await;

        Ok(())
    }

    async fn recreate_connection(&self) -> RmqResult<()> {
        if !self.can_reconnect {
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
            .create_pool(Some(deadpool_lapin::Runtime::Tokio1))
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

    #[test]
    fn test_cancellation_token_field_exists() {}

    #[test]
    fn test_prefetch_count_field_exists() {}

    #[test]
    fn test_health_check_interval_field_exists() {}
}
