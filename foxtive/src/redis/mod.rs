use crate::prelude::{AppResult, OnceLockHelper};
use crate::redis::conn::create_redis_connection;
use crate::results::redis_result::RedisResultToAppResult;
use crate::FOXTIVE;
use anyhow::Error;
use futures_util::StreamExt;
use log::{error, info};
use redis::{AsyncCommands, FromRedisValue, ToRedisArgs};
use serde::Serialize;
use std::future::Future;
use std::num::{NonZeroU64, NonZeroUsize};
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::time;

pub mod config;
pub mod conn;

pub struct Redis {
    pool: deadpool_redis::Pool,
}

impl Redis {
    pub fn new(pool: deadpool_redis::Pool) -> Self {
        Self { pool }
    }

    pub async fn redis(&self) -> AppResult<deadpool_redis::Connection> {
        self.pool.get().await.map_err(Error::msg)
    }

    /// Push a value to a Redis list
    pub async fn queue<T>(&self, queue: &str, data: &T) -> AppResult<i32>
    where
        T: ToRedisArgs + Send + Sync,
    {
        let mut conn = self.redis().await?;
        conn.lpush(queue, data).await.into_app_result()
    }

    pub async fn set<T>(&self, key: &str, value: &T) -> AppResult<String>
    where
        T: ToRedisArgs + Send + Sync,
    {
        let mut conn = self.redis().await?;
        conn.set(key, value).await.into_app_result()
    }

    pub async fn get<T: FromRedisValue>(&self, key: &str) -> AppResult<T> {
        let mut conn = self.redis().await?;
        conn.get(key).await.into_app_result()
    }

    pub async fn delete(&self, key: &str) -> AppResult<i32> {
        let mut conn = self.redis().await?;
        conn.del(key).await.into_app_result()
    }

    /// Delete Redis keys matching a pattern.
    ///
    /// # Arguments
    /// * `pattern` - The glob-style pattern to match keys (e.g. "my_prefix:*")
    ///
    /// # Returns
    /// * `AppResult<u32>` - The number of keys deleted
    pub async fn delete_by_pattern(&self, pattern: &str) -> AppResult<u32> {
        let mut conn = self.redis().await?;
        let keys: Vec<String> = conn.keys(pattern).await?;

        if keys.is_empty() {
            return Ok(0);
        }

        conn.del(keys).await.into_app_result()
    }

    pub async fn publish<T: Serialize>(&self, channel: &str, data: &T) -> AppResult<i32> {
        let content = serde_json::to_string(data)?;
        let mut conn = self.redis().await?;
        conn.publish(channel, content).await.into_app_result()
    }

    pub async fn rpop<V: FromRedisValue>(
        &self,
        key: &str,
        count: Option<NonZeroUsize>,
    ) -> AppResult<V> {
        let mut conn = self.redis().await?;
        conn.rpop(key, count).await.into_app_result()
    }

    // Right push (append to a list)
    pub async fn rpush<T: Serialize>(&self, queue: &str, data: &T) -> AppResult<i32> {
        let content = serde_json::to_string(data)?;
        let mut conn = self.redis().await?;
        conn.rpush(queue, content).await.into_app_result()
    }

    // Left pop (remove from the front of a list)
    pub async fn lpop<V: FromRedisValue>(
        &self,
        key: &str,
        count: Option<NonZeroUsize>,
    ) -> AppResult<V> {
        let mut conn = self.redis().await?;
        conn.lpop(key, count).await.into_app_result()
    }

    /// Add a value to a set
    pub async fn sadd<T: Serialize>(&self, key: &str, value: &T) -> AppResult<i32> {
        let content = serde_json::to_string(value)?;
        let mut conn = self.redis().await?;
        conn.sadd(key, content).await.into_app_result()
    }

    /// Pop a random element from a set
    pub async fn spop<V: FromRedisValue>(&self, key: &str) -> AppResult<V> {
        let mut conn = self.redis().await?;
        conn.spop(key).await.into_app_result()
    }

    /// Add a value to a sorted set with a score
    pub async fn zadd<T: Serialize>(&self, key: &str, score: f64, value: &T) -> AppResult<i32> {
        let content = serde_json::to_string(value)?;
        let mut conn = self.redis().await?;
        conn.zadd(key, score, content).await.into_app_result()
    }

    /// Pop the lowest scoring element from a sorted set
    pub async fn zpopmin(&self, key: &str, count: isize) -> AppResult<Option<(String, f64)>> {
        let mut conn = self.redis().await?;
        conn.zpopmin(key, count).await.into_app_result()
    }

    /// Pop the highest scoring element from a sorted set
    pub async fn zpopmax(&self, key: &str, count: isize) -> AppResult<Option<(String, f64)>> {
        let mut conn = self.redis().await?;
        conn.zpopmax(key, count).await.into_app_result()
    }

    /// Blocking left pop (waits if list is empty)
    pub async fn blpop<V: FromRedisValue>(&self, key: &str, timeout: f64) -> AppResult<V> {
        let mut conn = self.redis().await?;
        conn.blpop(key, timeout).await.into_app_result()
    }

    /// Blocking right pop (waits if list is empty)
    pub async fn brpop<V: FromRedisValue>(&self, key: &str, timeout: f64) -> AppResult<V> {
        let mut conn = self.redis().await?;
        conn.brpop(key, timeout).await.into_app_result()
    }

    /// Retrieve a range of elements from a list
    pub async fn lrange<T: FromRedisValue>(
        &self,
        key: &str,
        start: isize,
        stop: isize,
    ) -> AppResult<Vec<T>> {
        let mut conn = self.redis().await?;
        conn.lrange(key, start, stop).await.into_app_result()
    }

    /// Remove elements from a list
    pub async fn lrem<T: Serialize>(&self, key: &str, count: isize, value: &T) -> AppResult<i32> {
        let content = serde_json::to_string(value)?;
        let mut conn = self.redis().await?;
        conn.lrem(key, count, content).await.into_app_result()
    }

    /// Flush all keys in the database
    pub async fn flush_all(&self) -> AppResult<()> {
        let mut conn = self.redis().await?;
        redis::cmd("FLUSHALL")
            .query_async(&mut *conn)
            .await
            .into_app_result()
    }

    /// Flush all keys in the database
    pub async fn flush_db(&self) -> AppResult<()> {
        let mut conn = self.redis().await?;
        redis::cmd("FLUSHDB")
            .query_async(&mut *conn)
            .await
            .into_app_result()
    }

    /// Polls a Redis queue at a given interval and processes items using `func`
    ///
    /// # Arguments
    /// - `queue`: The Redis queue to poll
    /// - `interval`: The interval (in microseconds) between polls, defaults to 500ms
    /// - `len`: The number of items to retrieve per poll, defaults to 1
    /// - `func`: The async function to process each retrieved item
    ///
    /// # Example
    /// ```no_run
    /// use foxtive::redis::Redis;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     Redis::poll_queue("my_queue".to_string(), None, None, |item| async move {
    ///         println!("Processing item: {}", item);
    ///         Ok(())
    ///     }).await;
    /// }
    /// ```
    pub async fn poll_queue<F, Fut>(
        queue: String,
        interval: Option<NonZeroU64>,
        len: Option<NonZeroUsize>,
        mut func: F,
    ) where
        F: FnMut(String) -> Fut + Send + Copy + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        info!("[queue] polling: {queue}");
        let mut interval = time::interval(Duration::from_micros(
            interval.map(|v| v.get()).unwrap_or(500_000),
        ));

        loop {
            match FOXTIVE.redis().rpop(&queue, len).await {
                Ok(Some(item)) => {
                    let queue_clone = queue.clone();
                    Handle::current().spawn(async move {
                        if let Err(err) = func(item).await {
                            error!("[queue][{queue_clone}] executor error: {err:?}");
                        }
                    });
                }
                Ok(None) | Err(_) => {
                    interval.tick().await;
                }
            }
        }
    }

    /// Subscribes to a Redis channel and executes `func` on each message received
    ///
    /// **Note:** this method will establish new redis connection
    pub async fn subscribe<F, Fut>(channel: String, dns: String, mut func: F) -> AppResult<()>
    where
        F: FnMut(AppResult<String>) -> Fut + Copy + Send + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        info!("[subscriber] establishing connection...");
        let client = create_redis_connection(&dns)?;

        let mut pubsub = client.get_async_pubsub().await?;
        info!("[subscriber] subscribing to: {channel}");

        pubsub.subscribe(&[channel.clone()]).await?;
        let mut stream = pubsub.into_on_message();

        while let Some(msg) = stream.next().await {
            let channel_clone = channel.clone();
            Handle::current().spawn(async move {
                let received = msg.get_payload::<String>().into_app_result();
                if let Err(err) = func(received).await {
                    error!("[subscriber][{channel_clone}] executor error: {err:?}");
                }
            });
        }

        Ok(())
    }

    /// Returns all keys in the Redis database.
    ///
    /// This method uses Redis' KEYS command with a "*" pattern to retrieve all keys.
    /// Note: The KEYS command should be used with caution in production environments
    /// as it may impact performance for large datasets.
    ///
    /// # Returns
    /// - `AppResult<Vec<String>>`: A vector containing all keys in the database
    pub async fn keys(&self) -> AppResult<Vec<String>> {
        self.keys_by_pattern("*").await
    }

    /// Returns keys matching the specified pattern in the Redis database.
    ///
    /// This method uses Redis' KEYS command with the provided pattern.
    /// Supports Redis glob-style patterns:
    /// - `h?llo` matches `hello`, `hallo` and `hxllo`
    /// - `h*llo` matches `hllo` and `heeeello`
    /// - `h[ae]llo` matches `hello` and `hallo`, but not `hillo`
    ///
    /// # Arguments
    /// * `pattern` - Redis glob-style pattern to match against keys
    ///
    /// # Returns
    /// - `AppResult<Vec<String>>`: A vector containing all matching keys
    pub async fn keys_by_pattern(&self, pattern: &str) -> AppResult<Vec<String>> {
        let mut conn = self.redis().await?;
        conn.keys(pattern).await.into_app_result()
    }
}
