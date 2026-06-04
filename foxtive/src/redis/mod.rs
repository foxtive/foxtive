use crate::FOXTIVE;
use crate::prelude::{AppResult, AppStateExt};
use crate::redis::conn::create_redis_connection;
use crate::results::redis_result::RedisResultToAppResult;
use anyhow::Error;
use futures_util::StreamExt;
use redis::{AsyncCommands, FromRedisValue, ToRedisArgs, ToSingleRedisArg};
use serde::Serialize;
use std::collections::HashMap;
use std::future::Future;
use std::num::{NonZeroU64, NonZeroUsize};
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::time;
use tracing::{error, info};

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
        T: ToSingleRedisArg + Send + Sync,
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

    /// Retrieve a range of elements from a list
    pub async fn zrange<T: FromRedisValue>(
        &self,
        key: &str,
        start: isize,
        stop: isize,
    ) -> AppResult<Vec<T>> {
        let mut conn = self.redis().await?;
        conn.zrange(key, start, stop).await.into_app_result()
    }

    /// Return a range of members in a sorted set, by score with scores.
    pub async fn zrangebyscore_withscores<T, K, M, MM>(
        &self,
        key: K,
        min: M,
        max: MM,
    ) -> AppResult<Vec<T>>
    where
        T: FromRedisValue + Send + Sync,
        K: ToSingleRedisArg + Send + Sync,
        M: ToSingleRedisArg + Send + Sync,
        MM: ToSingleRedisArg + Send + Sync,
    {
        let mut conn = self.redis().await?;
        conn.zrangebyscore_withscores(key, min, max)
            .await
            .into_app_result()
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

        pubsub.subscribe(std::slice::from_ref(&channel)).await?;
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

    // String Operations

    /// Get the value of a key and set its old value.
    pub async fn getset<K: ToSingleRedisArg + Send + Sync, V: FromRedisValue>(
        &self,
        key: K,
        value: K,
    ) -> AppResult<Option<V>> {
        let mut conn = self.redis().await?;
        conn.getset(key, value).await.into_app_result()
    }

    /// Get a range of bytes/substring from the value of a key.
    pub async fn getrange<K: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        from: isize,
        to: isize,
    ) -> AppResult<String> {
        let mut conn = self.redis().await?;
        conn.getrange(key, from, to).await.into_app_result()
    }

    /// Overwrite the part of the value stored in key at the specified offset.
    pub async fn setrange<K: ToSingleRedisArg + Send + Sync, V: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        offset: isize,
        value: V,
    ) -> AppResult<usize> {
        let mut conn = self.redis().await?;
        conn.setrange(key, offset, value).await.into_app_result()
    }

    /// Append a value to a key.
    pub async fn append<K: ToSingleRedisArg + Send + Sync, V: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        value: V,
    ) -> AppResult<usize> {
        let mut conn = self.redis().await?;
        conn.append(key, value).await.into_app_result()
    }

    /// Increment the numeric value of a key by the given amount.
    pub async fn incr<K: ToSingleRedisArg + Send + Sync, V: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        delta: V,
    ) -> AppResult<isize> {
        let mut conn = self.redis().await?;
        conn.incr(key, delta).await.into_app_result()
    }

    /// Decrement the numeric value of a key by the given amount.
    pub async fn decr<K: ToSingleRedisArg + Send + Sync, V: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        delta: V,
    ) -> AppResult<isize> {
        let mut conn = self.redis().await?;
        conn.decr(key, delta).await.into_app_result()
    }

    /// Set the string value of a key with expiration in seconds.
    pub async fn set_ex<K: ToSingleRedisArg + Send + Sync, V: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        value: V,
        seconds: u64,
    ) -> AppResult<()> {
        let mut conn = self.redis().await?;
        conn.set_ex(key, value, seconds).await.into_app_result()
    }

    /// Set the string value of a key with expiration in milliseconds.
    pub async fn pset_ex<K: ToSingleRedisArg + Send + Sync, V: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        value: V,
        milliseconds: u64,
    ) -> AppResult<()> {
        let mut conn = self.redis().await?;
        conn.pset_ex(key, value, milliseconds)
            .await
            .into_app_result()
    }

    /// Set the value of a key, only if the key does not exist.
    pub async fn set_nx<K: ToSingleRedisArg + Send + Sync, V: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        value: V,
    ) -> AppResult<bool> {
        let mut conn = self.redis().await?;
        conn.set_nx(key, value).await.into_app_result()
    }

    /// Get the value of a key and delete it.
    pub async fn get_del<K: ToSingleRedisArg + Send + Sync, V: FromRedisValue>(
        &self,
        key: K,
    ) -> AppResult<Option<V>> {
        let mut conn = self.redis().await?;
        conn.get_del(key).await.into_app_result()
    }

    /// Rename a key.
    pub async fn rename<K: ToSingleRedisArg + Send + Sync, N: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        new_key: N,
    ) -> AppResult<()> {
        let mut conn = self.redis().await?;
        conn.rename(key, new_key).await.into_app_result()
    }

    /// Rename a key, only if the new key does not exist.
    pub async fn rename_nx<K: ToSingleRedisArg + Send + Sync, N: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        new_key: N,
    ) -> AppResult<bool> {
        let mut conn = self.redis().await?;
        conn.rename_nx(key, new_key).await.into_app_result()
    }

    /// Unlink one or more keys (non-blocking DEL).
    pub async fn unlink<K: ToRedisArgs + Send + Sync>(&self, key: K) -> AppResult<usize> {
        let mut conn = self.redis().await?;
        conn.unlink(key).await.into_app_result()
    }

    /// Determine if a key exists.
    pub async fn exists<K: ToRedisArgs + Send + Sync>(&self, key: K) -> AppResult<bool> {
        let mut conn = self.redis().await?;
        conn.exists(key).await.into_app_result()
    }

    /// Set a key's time to live in seconds.
    pub async fn expire<K: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        seconds: i64,
    ) -> AppResult<bool> {
        let mut conn = self.redis().await?;
        conn.expire(key, seconds).await.into_app_result()
    }

    /// Set the expiration for a key as a UNIX timestamp.
    pub async fn expire_at<K: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        ts: i64,
    ) -> AppResult<bool> {
        let mut conn = self.redis().await?;
        conn.expire_at(key, ts).await.into_app_result()
    }

    /// Set a key's time to live in milliseconds.
    pub async fn pexpire<K: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        ms: i64,
    ) -> AppResult<bool> {
        let mut conn = self.redis().await?;
        conn.pexpire(key, ms).await.into_app_result()
    }

    /// Remove the expiration from a key.
    pub async fn persist<K: ToSingleRedisArg + Send + Sync>(&self, key: K) -> AppResult<bool> {
        let mut conn = self.redis().await?;
        conn.persist(key).await.into_app_result()
    }

    /// Get the time to live for a key in seconds.
    pub async fn ttl<K: ToSingleRedisArg + Send + Sync>(&self, key: K) -> AppResult<i64> {
        let mut conn = self.redis().await?;
        conn.ttl(key).await.into_app_result()
    }

    /// Get the time to live for a key in milliseconds.
    pub async fn pttl<K: ToSingleRedisArg + Send + Sync>(&self, key: K) -> AppResult<i64> {
        let mut conn = self.redis().await?;
        conn.pttl(key).await.into_app_result()
    }

    /// Get the length of the value stored in a key.
    pub async fn strlen<K: ToSingleRedisArg + Send + Sync>(&self, key: K) -> AppResult<usize> {
        let mut conn = self.redis().await?;
        conn.strlen(key).await.into_app_result()
    }

    // Hash Operations

    /// Gets a single field from a hash.
    pub async fn hget<
        K: ToSingleRedisArg + Send + Sync,
        F: ToSingleRedisArg + Send + Sync,
        V: FromRedisValue,
    >(
        &self,
        key: K,
        field: F,
    ) -> AppResult<Option<V>> {
        let mut conn = self.redis().await?;
        conn.hget(key, field).await.into_app_result()
    }

    /// Gets multiple fields from a hash.
    pub async fn hmget<
        K: ToSingleRedisArg + Send + Sync,
        F: ToRedisArgs + Send + Sync,
        V: FromRedisValue,
    >(
        &self,
        key: K,
        fields: F,
    ) -> AppResult<Vec<V>> {
        let mut conn = self.redis().await?;
        conn.hmget(key, fields).await.into_app_result()
    }

    /// Deletes a single field from a hash.
    pub async fn hdel<K: ToSingleRedisArg + Send + Sync, F: ToRedisArgs + Send + Sync>(
        &self,
        key: K,
        field: F,
    ) -> AppResult<usize> {
        let mut conn = self.redis().await?;
        conn.hdel(key, field).await.into_app_result()
    }

    /// Sets a single field in a hash.
    pub async fn hset<
        K: ToSingleRedisArg + Send + Sync,
        F: ToSingleRedisArg + Send + Sync,
        V: ToSingleRedisArg + Send + Sync,
    >(
        &self,
        key: K,
        field: F,
        value: V,
    ) -> AppResult<usize> {
        let mut conn = self.redis().await?;
        conn.hset(key, field, value).await.into_app_result()
    }

    /// Sets a single field in a hash if it does not exist.
    pub async fn hset_nx<
        K: ToSingleRedisArg + Send + Sync,
        F: ToSingleRedisArg + Send + Sync,
        V: ToSingleRedisArg + Send + Sync,
    >(
        &self,
        key: K,
        field: F,
        value: V,
    ) -> AppResult<bool> {
        let mut conn = self.redis().await?;
        conn.hset_nx(key, field, value).await.into_app_result()
    }

    /// Checks if a field in a hash exists.
    pub async fn hexists<K: ToSingleRedisArg + Send + Sync, F: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        field: F,
    ) -> AppResult<bool> {
        let mut conn = self.redis().await?;
        conn.hexists(key, field).await.into_app_result()
    }

    /// Get all the keys in a hash.
    pub async fn hkeys<K: ToSingleRedisArg + Send + Sync>(&self, key: K) -> AppResult<Vec<String>> {
        let mut conn = self.redis().await?;
        conn.hkeys(key).await.into_app_result()
    }

    /// Get all the values in a hash.
    pub async fn hvals<K: ToSingleRedisArg + Send + Sync>(&self, key: K) -> AppResult<Vec<String>> {
        let mut conn = self.redis().await?;
        conn.hvals(key).await.into_app_result()
    }

    /// Get all the fields and values in a hash.
    pub async fn hgetall<K: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
    ) -> AppResult<HashMap<String, String>> {
        let mut conn = self.redis().await?;
        conn.hgetall(key).await.into_app_result()
    }

    /// Get the length of a hash.
    pub async fn hlen<K: ToSingleRedisArg + Send + Sync>(&self, key: K) -> AppResult<usize> {
        let mut conn = self.redis().await?;
        conn.hlen(key).await.into_app_result()
    }

    /// Increments a value in a hash.
    pub async fn hincr<
        K: ToSingleRedisArg + Send + Sync,
        F: ToSingleRedisArg + Send + Sync,
        D: ToSingleRedisArg + Send + Sync,
    >(
        &self,
        key: K,
        field: F,
        delta: D,
    ) -> AppResult<f64> {
        let mut conn = self.redis().await?;
        conn.hincr(key, field, delta).await.into_app_result()
    }

    // List Operations

    /// Get the length of a list.
    pub async fn llen<K: ToSingleRedisArg + Send + Sync>(&self, key: K) -> AppResult<usize> {
        let mut conn = self.redis().await?;
        conn.llen(key).await.into_app_result()
    }

    /// Get an element from a list by its index.
    pub async fn lindex<K: ToSingleRedisArg + Send + Sync, V: FromRedisValue>(
        &self,
        key: K,
        index: isize,
    ) -> AppResult<Option<V>> {
        let mut conn = self.redis().await?;
        conn.lindex(key, index).await.into_app_result()
    }

    /// Insert an element before another element in a list.
    pub async fn linsert_before<
        K: ToSingleRedisArg + Send + Sync,
        P: ToSingleRedisArg + Send + Sync,
        V: ToSingleRedisArg + Send + Sync,
    >(
        &self,
        key: K,
        pivot: P,
        value: V,
    ) -> AppResult<isize> {
        let mut conn = self.redis().await?;
        conn.linsert_before(key, pivot, value)
            .await
            .into_app_result()
    }

    /// Insert an element after another element in a list.
    pub async fn linsert_after<
        K: ToSingleRedisArg + Send + Sync,
        P: ToSingleRedisArg + Send + Sync,
        V: ToSingleRedisArg + Send + Sync,
    >(
        &self,
        key: K,
        pivot: P,
        value: V,
    ) -> AppResult<isize> {
        let mut conn = self.redis().await?;
        conn.linsert_after(key, pivot, value)
            .await
            .into_app_result()
    }

    /// Insert all the specified values at the head of the list stored at key.
    pub async fn lpush<K: ToSingleRedisArg + Send + Sync, V: ToRedisArgs + Send + Sync>(
        &self,
        key: K,
        value: V,
    ) -> AppResult<usize> {
        let mut conn = self.redis().await?;
        conn.lpush(key, value).await.into_app_result()
    }

    /// Trim an existing list so that it will contain only the specified range of elements.
    pub async fn ltrim<K: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        start: isize,
        stop: isize,
    ) -> AppResult<()> {
        let mut conn = self.redis().await?;
        conn.ltrim(key, start, stop).await.into_app_result()
    }

    /// Set the list element at index to value.
    pub async fn lset<K: ToSingleRedisArg + Send + Sync, V: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        index: isize,
        value: V,
    ) -> AppResult<()> {
        let mut conn = self.redis().await?;
        conn.lset(key, index, value).await.into_app_result()
    }

    // Set Operations

    /// Get the number of members in a set.
    pub async fn scard<K: ToSingleRedisArg + Send + Sync>(&self, key: K) -> AppResult<usize> {
        let mut conn = self.redis().await?;
        conn.scard(key).await.into_app_result()
    }

    /// Determine if a given value is a member of a set.
    pub async fn sismember<K: ToSingleRedisArg + Send + Sync, M: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        member: M,
    ) -> AppResult<bool> {
        let mut conn = self.redis().await?;
        conn.sismember(key, member).await.into_app_result()
    }

    /// Get all the members in a set.
    pub async fn smembers<K: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
    ) -> AppResult<Vec<String>> {
        let mut conn = self.redis().await?;
        conn.smembers(key).await.into_app_result()
    }

    /// Remove one or more members from a set.
    pub async fn srem<K: ToSingleRedisArg + Send + Sync, M: ToRedisArgs + Send + Sync>(
        &self,
        key: K,
        member: M,
    ) -> AppResult<usize> {
        let mut conn = self.redis().await?;
        conn.srem(key, member).await.into_app_result()
    }

    /// Get one random member from a set.
    pub async fn srandmember<K: ToSingleRedisArg + Send + Sync, V: FromRedisValue>(
        &self,
        key: K,
    ) -> AppResult<Option<V>> {
        let mut conn = self.redis().await?;
        conn.srandmember(key).await.into_app_result()
    }

    // Sorted Set Operations

    /// Get the number of members in a sorted set.
    pub async fn zcard<K: ToSingleRedisArg + Send + Sync>(&self, key: K) -> AppResult<usize> {
        let mut conn = self.redis().await?;
        conn.zcard(key).await.into_app_result()
    }

    /// Count the members in a sorted set with scores within the given values.
    pub async fn zcount<
        K: ToSingleRedisArg + Send + Sync,
        M: ToSingleRedisArg + Send + Sync,
        MM: ToSingleRedisArg + Send + Sync,
    >(
        &self,
        key: K,
        min: M,
        max: MM,
    ) -> AppResult<usize> {
        let mut conn = self.redis().await?;
        conn.zcount(key, min, max).await.into_app_result()
    }

    /// Increments the member in a sorted set at key by delta.
    pub async fn zincr<
        K: ToSingleRedisArg + Send + Sync,
        M: ToSingleRedisArg + Send + Sync,
        D: ToSingleRedisArg + Send + Sync,
    >(
        &self,
        key: K,
        member: M,
        delta: D,
    ) -> AppResult<f64> {
        let mut conn = self.redis().await?;
        conn.zincr(key, member, delta).await.into_app_result()
    }

    /// Get the score associated with the given member in a sorted set.
    pub async fn zscore<K: ToSingleRedisArg + Send + Sync, M: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        member: M,
    ) -> AppResult<Option<f64>> {
        let mut conn = self.redis().await?;
        conn.zscore(key, member).await.into_app_result()
    }

    /// Determine the index of a member in a sorted set.
    pub async fn zrank<K: ToSingleRedisArg + Send + Sync, M: ToSingleRedisArg + Send + Sync>(
        &self,
        key: K,
        member: M,
    ) -> AppResult<Option<usize>> {
        let mut conn = self.redis().await?;
        conn.zrank(key, member).await.into_app_result()
    }

    /// Remove one or more members from a sorted set.
    pub async fn zrem<K: ToSingleRedisArg + Send + Sync, M: ToRedisArgs + Send + Sync>(
        &self,
        key: K,
        members: M,
    ) -> AppResult<usize> {
        let mut conn = self.redis().await?;
        conn.zrem(key, members).await.into_app_result()
    }

    // Server Operations

    /// Sends a ping to the server.
    pub async fn ping(&self) -> AppResult<String> {
        let mut conn = self.redis().await?;
        conn.ping().await.into_app_result()
    }

    /// Returns the number of keys in the currently selected database.
    pub async fn dbsize(&self) -> AppResult<usize> {
        let mut conn = self.redis().await?;
        redis::cmd("DBSIZE")
            .query_async(&mut *conn)
            .await
            .into_app_result()
    }
}
