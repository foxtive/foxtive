use crate::prelude::AppResult;
use async_trait::async_trait;
use log::{debug, error};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::future::Future;

#[async_trait::async_trait]
pub trait CacheDriverContract: Send + Sync {
    async fn put_raw(&self, key: &str, value: String) -> AppResult<String>;

    async fn get_raw(&self, key: &str) -> AppResult<Option<String>>;

    async fn forget(&self, key: &str) -> AppResult<i32>;
}

#[async_trait]
pub trait CacheDriverExt {
    async fn put<T>(&self, key: &str, value: &T) -> AppResult<String>
    where
        T: Serialize + Sync;

    async fn get<T>(&self, key: &str) -> AppResult<Option<T>>
    where
        T: DeserializeOwned + Sync;

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
            debug!("'{}' collected from cache :)", key);
            return Ok(val);
        }

        debug!("'{}' is missing in cache, executing setter()...", key);

        let val = setter().await?;

        // Store the value before returning to ensure cache consistency
        if let Err(e) = self.put(key, &val).await {
            error!("Failed to cache value for '{}': {:?}", key, e);
            return Err(e);
        }

        Ok(val)
    }
}
