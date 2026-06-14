use crate::prelude::{RabbitMQ, RmqError, RmqResult};
use lapin::types::LongInt;
use lapin::BasicProperties;
use std::time::Duration;
use tracing::{debug, error};

/// Builder for publishing messages with flexible options.
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
///         .header("service_name", lapin::types::AMQPValue::LongString(
///             lapin::types::LongString::from("user-service")
///         ))
///         .header("correlation_id", lapin::types::AMQPValue::LongString(
///             lapin::types::LongString::from("abc-123")
///         ))
///         .send().await.unwrap();
/// }
/// ```
pub struct MessagePublisher<'a> {
    rabbitmq: &'a mut RabbitMQ,
    exchange: Option<String>,
    routing_key: Option<String>,
    payload: Option<Vec<u8>>,
    properties: BasicProperties,
    delay: Option<Duration>,
}

impl<'a> MessagePublisher<'a> {
    pub(crate) fn new(rabbitmq: &'a mut RabbitMQ) -> Self {
        Self {
            rabbitmq,
            exchange: None,
            routing_key: None,
            payload: None,
            properties: BasicProperties::default(),
            delay: None,
        }
    }

    /// Set the exchange name
    pub fn exchange(mut self, exchange: impl ToString) -> Self {
        self.exchange = Some(exchange.to_string());
        self
    }

    /// Set the routing key
    pub fn routing_key(mut self, routing_key: impl ToString) -> Self {
        self.routing_key = Some(routing_key.to_string());
        self
    }

    /// Set the message payload
    pub fn payload(mut self, payload: impl AsRef<[u8]>) -> Self {
        self.payload = Some(payload.as_ref().to_vec());
        self
    }

    /// Set custom BasicProperties (replaces existing properties)
    pub fn properties(mut self, properties: BasicProperties) -> Self {
        self.properties = properties;
        self
    }

    /// Add a header to the message
    pub fn header(mut self, key: impl ToString, value: impl Into<lapin::types::AMQPValue>) -> Self {
        let mut headers = self
            .properties
            .headers()
            .clone()
            .unwrap_or_default();
        headers.insert(
            lapin::types::ShortString::from(key.to_string()),
            value.into(),
        );
        self.properties = self.properties.with_headers(headers);
        self
    }

    /// Set message delay (requires rabbitmq-delayed-message-exchange plugin)
    pub fn delay(mut self, duration: Duration) -> Self {
        self.delay = Some(duration);
        self
    }

    /// Set content type
    pub fn content_type(mut self, content_type: impl ToString) -> Self {
        self.properties = self
            .properties
            .with_content_type(lapin::types::ShortString::from(content_type.to_string()));
        self
    }

    /// Set correlation ID
    pub fn correlation_id(mut self, correlation_id: impl ToString) -> Self {
        self.properties = self
            .properties
            .with_correlation_id(lapin::types::ShortString::from(correlation_id.to_string()));
        self
    }

    /// Set message ID
    pub fn message_id(mut self, message_id: impl ToString) -> Self {
        self.properties = self
            .properties
            .with_message_id(lapin::types::ShortString::from(message_id.to_string()));
        self
    }

    /// Send the message with all configured options
    pub async fn send(self) -> RmqResult<()> {
        let exchange = self.exchange.ok_or_else(|| RmqError::Configuration {
            message: "Exchange is required".to_string(),
        })?;

        let routing_key = self.routing_key.ok_or_else(|| RmqError::Configuration {
            message: "Routing key is required".to_string(),
        })?;

        let payload = self.payload.ok_or_else(|| RmqError::Configuration {
            message: "Payload is required".to_string(),
        })?;

        // Apply delay if configured
        let mut final_props = self.properties;
        if let Some(delay) = self.delay {
            let delay_ms = delay.as_millis() as i64;
            let mut headers = final_props.headers().clone().unwrap_or_default();
            headers.insert(
                lapin::types::ShortString::from("x-delay"),
                lapin::types::AMQPValue::LongInt(delay_ms as LongInt),
            );
            final_props = final_props.with_headers(headers);
            debug!(
                "Published delayed message to '{}' with {}ms delay",
                exchange, delay_ms
            );
        }

        self.rabbitmq.ensure_channel_is_usable(true).await?;

        tokio::time::timeout(
            self.rabbitmq.operation_timeout,
            self.rabbitmq.publish_channel.basic_publish(
                &exchange,
                &routing_key,
                self.rabbitmq.default_publish_options,
                &payload,
                final_props,
            ),
        )
        .await
        .map_err(|_| RmqError::timeout("basic_publish", self.rabbitmq.operation_timeout))?
        .inspect_err(|e| error!("Failed to publish message: {e:?}"))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use lapin::types::FieldTable;
    // #[test]
    // fn test_message_publisher_creation() {
    //     // This test verifies the builder can be created (requires mock RabbitMQ)
    //     // We're testing the API structure, not actual publishing
    //     assert!(true); // Placeholder - actual integration tests need RabbitMQ running
    // }
    //
    // #[test]
    // fn test_builder_pattern_chaining() {
    //     // Verify that all builder methods return Self for chaining
    //     // This is a compile-time check - if it compiles, chaining works
    //     assert!(true);
    // }

    #[tokio::test]
    async fn test_publisher_requires_exchange() {
        // Test that send() fails without exchange
        let config = deadpool_lapin::Config {
            url: Some("amqp://localhost:5672".to_string()),
            ..Default::default()
        };
        let pool = config
            .create_pool(Some(deadpool_lapin::Runtime::Tokio1))
            .unwrap();

        let mut rmq = match RabbitMQ::new(pool).await {
            Ok(rmq) => rmq,
            Err(_) => {
                // If connection fails (no RabbitMQ), skip this test
                return;
            }
        };

        let result = rmq
            .publisher()
            .routing_key("test.key")
            .payload(b"test")
            .send()
            .await;

        assert!(result.is_err());
        if let Err(RmqError::Configuration { message }) = result {
            assert_eq!(message, "Exchange is required");
        }
    }

    #[tokio::test]
    async fn test_publisher_requires_routing_key() {
        let config = deadpool_lapin::Config {
            url: Some("amqp://localhost:5672".to_string()),
            ..Default::default()
        };
        let pool = config
            .create_pool(Some(deadpool_lapin::Runtime::Tokio1))
            .unwrap();

        let mut rmq = match RabbitMQ::new(pool).await {
            Ok(rmq) => rmq,
            Err(_) => return,
        };

        let result = rmq
            .publisher()
            .exchange("test-exchange")
            .payload(b"test")
            .send()
            .await;

        assert!(result.is_err());
        if let Err(RmqError::Configuration { message }) = result {
            assert_eq!(message, "Routing key is required");
        }
    }

    #[tokio::test]
    async fn test_publisher_requires_payload() {
        let config = deadpool_lapin::Config {
            url: Some("amqp://localhost:5672".to_string()),
            ..Default::default()
        };
        let pool = config
            .create_pool(Some(deadpool_lapin::Runtime::Tokio1))
            .unwrap();

        let mut rmq = match RabbitMQ::new(pool).await {
            Ok(rmq) => rmq,
            Err(_) => return,
        };

        let result = rmq
            .publisher()
            .exchange("test-exchange")
            .routing_key("test.key")
            .send()
            .await;

        assert!(result.is_err());
        if let Err(RmqError::Configuration { message }) = result {
            assert_eq!(message, "Payload is required");
        }
    }

    #[test]
    fn test_header_addition() {
        // Test that headers can be added to properties
        let props = BasicProperties::default();
        let mut headers = FieldTable::default();
        headers.insert(
            lapin::types::ShortString::from("test_key"),
            lapin::types::AMQPValue::LongString(lapin::types::LongString::from("test_value")),
        );
        let props_with_headers = props.with_headers(headers);

        assert!(props_with_headers.headers().is_some());
    }

    #[test]
    fn test_delay_conversion() {
        // Test that delay duration is correctly converted to milliseconds
        let delay = Duration::from_secs(5);
        let delay_ms = delay.as_millis() as i64;
        assert_eq!(delay_ms, 5000);

        let delay = Duration::from_millis(1500);
        let delay_ms = delay.as_millis() as i64;
        assert_eq!(delay_ms, 1500);
    }

    #[test]
    fn test_content_type_property() {
        let props = BasicProperties::default()
            .with_content_type(lapin::types::ShortString::from("application/json"));

        let content_type = props.content_type().clone();

        assert!(content_type.is_some());
        assert_eq!(content_type.unwrap().as_str(), "application/json");
    }

    #[test]
    fn test_correlation_id_property() {
        let props = BasicProperties::default()
            .with_correlation_id(lapin::types::ShortString::from("corr-123"));

        let corr_id = props.correlation_id().clone();

        assert!(corr_id.is_some());
        assert_eq!(corr_id.unwrap().as_str(), "corr-123");
    }

    #[test]
    fn test_message_id_property() {
        let props =
            BasicProperties::default().with_message_id(lapin::types::ShortString::from("msg-456"));
        let msg_id = props.message_id().clone();

        assert!(msg_id.is_some());
        assert_eq!(msg_id.unwrap().as_str(), "msg-456");
    }

    #[test]
    fn test_multiple_headers() {
        let mut headers = FieldTable::default();
        headers.insert(
            lapin::types::ShortString::from("service_name"),
            lapin::types::AMQPValue::LongString(lapin::types::LongString::from("user-service")),
        );
        headers.insert(
            lapin::types::ShortString::from("correlation_id"),
            lapin::types::AMQPValue::LongString(lapin::types::LongString::from("abc-123")),
        );
        headers.insert(
            lapin::types::ShortString::from("priority"),
            lapin::types::AMQPValue::LongInt(1),
        );

        assert_eq!(headers.inner().len(), 3);
    }
}
