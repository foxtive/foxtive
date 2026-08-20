use crate::prelude::AppResult;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::future::Future;
use std::pin::Pin;
use tracing::{debug, error};

/// Contract for implementing cache storage drivers
pub trait CacheDriverContract: Send + Sync {
    /// Retrieves all keys present in the cache
    fn keys(&self) -> Pin<Box<dyn Future<Output = AppResult<Vec<String>>> + Send + '_>>;

    /// Retrieves all keys matching the specified pattern
    fn keys_by_pattern(
        &self,
        pattern: &str,
    ) -> Pin<Box<dyn Future<Output = AppResult<Vec<String>>> + Send + '_>>;

    /// Stores a raw string value in the cache
    fn put_raw(
        &self,
        key: &str,
        value: String,
    ) -> Pin<Box<dyn Future<Output = AppResult<String>> + Send + '_>>;

    /// Retrieves a raw string value from the cache
    fn get_raw(
        &self,
        key: &str,
    ) -> Pin<Box<dyn Future<Output = AppResult<Option<String>>> + Send + '_>>;

    /// Removes a single key from the cache
    fn forget(&self, key: &str) -> Pin<Box<dyn Future<Output = AppResult<i32>> + Send + '_>>;

    /// Removes all keys matching the specified pattern
    fn forget_by_pattern(
        &self,
        pattern: &str,
    ) -> Pin<Box<dyn Future<Output = AppResult<i32>> + Send + '_>>;
}

/// Extension trait providing serialization-aware caching operations
pub trait CacheDriverExt: CacheDriverContract {
    /// Stores a serializable value in the cache
    fn put<'a, T>(
        &'a self,
        key: &'a str,
        value: &'a T,
    ) -> Pin<Box<dyn Future<Output = AppResult<String>> + Send + 'a>>
    where
        T: Serialize + Sync + 'a;

    /// Retrieves and deserializes a value from the cache
    fn get<'a, T>(
        &'a self,
        key: &'a str,
    ) -> Pin<Box<dyn Future<Output = AppResult<Option<T>>> + Send + 'a>>
    where
        T: DeserializeOwned + Sync + 'a;

    /// Gets a value from cache or computes and stores it if missing
    fn get_or_put<'a, Val, Fun, Fut>(
        &'a self,
        key: &'a str,
        setter: Fun,
    ) -> Pin<Box<dyn Future<Output = AppResult<Val>> + Send + 'a>>
    where
        Val: Serialize + DeserializeOwned + Clone + Sync + Send + 'a,
        Fun: FnOnce() -> Fut + Send + 'a,
        Fut: Future<Output = AppResult<Val>> + Send + 'a;
}

impl<T: ?Sized + CacheDriverContract + Sync> CacheDriverExt for T {
    fn put<'a, U>(
        &'a self,
        key: &'a str,
        value: &'a U,
    ) -> Pin<Box<dyn Future<Output = AppResult<String>> + Send + 'a>>
    where
        U: Serialize + Sync + 'a,
    {
        Box::pin(async move {
            let json = serde_json::to_string(value)?;
            self.put_raw(key, json).await
        })
    }

    fn get<'a, U>(
        &'a self,
        key: &'a str,
    ) -> Pin<Box<dyn Future<Output = AppResult<Option<U>>> + Send + 'a>>
    where
        U: DeserializeOwned + Sync + 'a,
    {
        Box::pin(async move {
            let raw = self.get_raw(key).await?;
            Ok(match raw {
                None => None,
                Some(bytes) => Some(serde_json::from_str(&bytes)?),
            })
        })
    }

    fn get_or_put<'a, Val, Fun, Fut>(
        &'a self,
        key: &'a str,
        setter: Fun,
    ) -> Pin<Box<dyn Future<Output = AppResult<Val>> + Send + 'a>>
    where
        Val: Serialize + DeserializeOwned + Clone + Sync + Send + 'a,
        Fun: FnOnce() -> Fut + Send + 'a,
        Fut: Future<Output = AppResult<Val>> + Send + 'a,
    {
        Box::pin(async move {
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
        })
    }
}
