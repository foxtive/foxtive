use crate::prelude::AppResult;
use async_trait::async_trait;
use log::{debug, error};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::future::Future;

/// Contract for implementing cache storage drivers
#[async_trait::async_trait]
pub trait CacheDriverContract: Send + Sync {
    /// Retrieves all keys present in the cache
    ///
    /// # Returns
    /// - `AppResult<Vec<String>>`: A vector containing all cache keys
    async fn keys(&self) -> AppResult<Vec<String>>;

    /// Retrieves all keys matching the specified pattern
    ///
    /// # Parameters
    /// - `pattern`: Pattern string to match keys against
    ///
    /// # Returns
    /// - `AppResult<Vec<String>>`: A vector of matching cache keys
    async fn keys_by_pattern(&self, pattern: &str) -> AppResult<Vec<String>>;

    /// Stores a raw string value in the cache
    ///
    /// # Parameters
    /// - `key`: Cache key to store the value under
    /// - `value`: String value to store
    ///
    /// # Returns
    /// - `AppResult<String>`: The stored string value
    async fn put_raw(&self, key: &str, value: String) -> AppResult<String>;

    /// Retrieves a raw string value from the cache
    ///
    /// # Parameters
    /// - `key`: Cache key to retrieve
    ///
    /// # Returns
    /// - `AppResult<Option<String>>`: The stored string value if it exists
    async fn get_raw(&self, key: &str) -> AppResult<Option<String>>;

    /// Removes a single key from the cache
    ///
    /// # Parameters
    /// - `key`: Cache key to remove
    ///
    /// # Returns
    /// - `AppResult<i32>`: Number of keys removed (typically 1 or 0)
    async fn forget(&self, key: &str) -> AppResult<i32>;

    /// Removes all keys matching the specified pattern
    ///
    /// # Parameters
    /// - `pattern`: Pattern string to match keys for removal
    ///
    /// # Returns
    /// - `AppResult<i32>`: Number of keys removed
    async fn forget_by_pattern(&self, pattern: &str) -> AppResult<i32>;
}

/// Extension trait providing serialization-aware caching operations
#[async_trait]
pub trait CacheDriverExt {
    /// Stores a serializable value in the cache
    ///
    /// # Parameters
    /// - `key`: Cache key to store the value under
    /// - `value`: Value to serialize and store
    ///
    /// # Returns
    /// - `AppResult<String>`: The stored JSON string
    ///
    /// # Type Parameters
    /// - `T`: Type that implements Serialize + Sync
    async fn put<T>(&self, key: &str, value: &T) -> AppResult<String>
    where
        T: Serialize + Sync;

    /// Retrieves and deserializes a value from the cache
    ///
    /// # Parameters
    /// - `key`: Cache key to retrieve
    ///
    /// # Returns
    /// - `AppResult<Option<T>>`: The deserialized value if it exists
    ///
    /// # Type Parameters
    /// - `T`: Type that implements DeserializeOwned + Sync
    async fn get<T>(&self, key: &str) -> AppResult<Option<T>>
    where
        T: DeserializeOwned + Sync;

    /// Gets a value from cache or computes and stores it if missing
    ///
    /// # Parameters
    /// - `key`: Cache key to retrieve or store under
    /// - `setter`: Function to compute the value if not in cache
    ///
    /// # Returns
    /// - `AppResult<Val>`: The retrieved or computed value
    ///
    /// # Type Parameters
    /// - `Val`: The value type that implements required traits
    /// - `Fun`: The setter function type
    /// - `Fut`: The future returned by the setter
    async fn get_or_put<Val, Fun, Fut>(&self, key: &str, setter: Fun) -> AppResult<Val>
    where
        Val: Serialize + DeserializeOwned + Clone + Sync + Send,
        Fun: FnOnce() -> Fut + Send,
        Fut: Future<Output = AppResult<Val>> + Send;
}

#[async_trait]
impl<T: ?Sized + CacheDriverContract + Sync> CacheDriverExt for T {
    async fn put<U>(&self, key: &str, value: &U) -> AppResult<String>
    where
        U: Serialize + Sync,
    {
        let json = serde_json::to_string(value)?;
        self.put_raw(key, json).await
    }

    async fn get<U>(&self, key: &str) -> AppResult<Option<U>>
    where
        U: DeserializeOwned + Sync,
    {
        let raw = self.get_raw(key).await?;
        Ok(match raw {
            None => None,
            Some(bytes) => Some(serde_json::from_str(&bytes)?),
        })
    }

    async fn get_or_put<Val, Fun, Fut>(&self, key: &str, setter: Fun) -> AppResult<Val>
    where
        Val: Serialize + DeserializeOwned + Sync + Send, // Removed Clone requirement
        Fun: FnOnce() -> Fut + Send,
        Fut: Future<Output = AppResult<Val>> + Send,
    {
        if let Some(val) = self.get::<Val>(key).await? {
            debug!("'{key}' collected from cache :)");
            return Ok(val);
        }

        debug!("'{key}' is missing in cache, executing setter()...");

        let val = setter().await?;

        // Store the value before returning to ensure cache consistency
        if let Err(e) = self.put(key, &val).await {
            error!("Failed to cache value for '{key}': {e:?}");
            return Err(e);
        }

        Ok(val)
    }
}
