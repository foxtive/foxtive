use crate::prelude::RmqResult;
use lapin::message::Delivery;
use lapin::options::{BasicAckOptions, BasicNackOptions};
use lapin::types::{DeliveryTag, ShortString};

pub struct Message {
    delivery: Delivery,
}

impl Message {
    pub fn new(delivery: Delivery) -> Self {
        Self { delivery }
    }

    pub fn delivery(&self) -> &Delivery {
        &self.delivery
    }

    pub fn data(&self) -> &Vec<u8> {
        &self.delivery.data
    }

    pub fn str(&self) -> RmqResult<&str> {
        Ok(std::str::from_utf8(&self.delivery.data)?)
    }

    pub fn delivery_tag(&self) -> &DeliveryTag {
        &self.delivery.delivery_tag
    }

    pub fn routing_key(&self) -> &ShortString {
        &self.delivery.routing_key
    }

    /// Get message properties (headers, metadata, etc.)
    ///
    /// This allows you to access custom metadata attached to messages,
    /// such as service identification, correlation IDs, and other contextual information.
    pub fn properties(&self) -> &lapin::BasicProperties {
        &self.delivery.properties
    }

    /// Get a specific header value from message properties
    ///
    /// # Arguments
    /// * `key` - Header key to retrieve
    ///
    /// # Returns
    /// * `Some(AMQPValue)` if the header exists
    /// * `None` if the header doesn't exist or properties are not set
    pub fn get_header(&self, key: &str) -> Option<&lapin::types::AMQPValue> {
        self.delivery
            .properties
            .headers()
            .as_ref()
            .and_then(|headers| headers.inner().get(key))
    }

    /// Get a string header value
    ///
    /// # Arguments
    /// * `key` - Header key to retrieve
    ///
    /// # Returns
    /// * `Some(String)` if the header exists and is a valid string
    /// * `None` if the header doesn't exist or is not a string type
    pub fn get_string_header(&self, key: &str) -> Option<String> {
        self.get_header(key).and_then(|value| match value {
            lapin::types::AMQPValue::LongString(s) => Some(s.to_string()),
            lapin::types::AMQPValue::ShortString(s) => Some(s.to_string()),
            _ => None,
        })
    }

    pub fn deserialize<T>(&self) -> RmqResult<T>
    where
        T: serde::de::DeserializeOwned,
    {
        Ok(serde_json::from_slice(&self.delivery.data)?)
    }

    pub async fn ack(&self) -> RmqResult<()> {
        self.ack_opt(BasicAckOptions::default()).await?;
        Ok(())
    }

    pub async fn nack(&self) -> RmqResult<()> {
        self.nack_opt(BasicNackOptions::default()).await
    }

    pub async fn ack_opt(&self, opt: BasicAckOptions) -> RmqResult<()> {
        self.delivery.acker.ack(opt).await?;
        Ok(())
    }

    pub async fn nack_opt(&self, opt: BasicNackOptions) -> RmqResult<()> {
        self.delivery.acker.nack(opt).await?;
        Ok(())
    }
}
