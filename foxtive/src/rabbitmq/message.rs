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
