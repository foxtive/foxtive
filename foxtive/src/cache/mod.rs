pub mod contract;
pub mod drivers;

use crate::cache::contract::{CacheDriverContract, CacheDriverExt};
use crate::prelude::AppResult;
use serde::{de::DeserializeOwned, Serialize};
use std::future::Future;
use std::sync::Arc;

pub struct Cache {
    driver: Arc<dyn CacheDriverContract>,
}

impl Cache {
    pub fn new(driver: Arc<dyn CacheDriverContract>) -> Self {
        Self { driver }
    }

    pub fn driver(&self) -> Arc<dyn CacheDriverContract> {
        Arc::clone(&self.driver)
    }

    pub async fn put<T>(&self, key: &str, value: &T) -> AppResult<String>
    where
        T: Serialize + Sync,
    {
        self.driver.put_raw(key, serde_json::to_string(value)?).await
    }

    pub async fn get<T>(&self, key: &str) -> AppResult<Option<T>>
    where
        T: DeserializeOwned + Sync,
    {
        let raw = self.driver.get_raw(key).await?;

        match raw {
            Some(json) => {
                let deserialized = serde_json::from_str::<T>(&json).map_err(crate::Error::msg)?;
                Ok(Some(deserialized))
            }
            None => Ok(None),
        }
    }

    pub async fn forget(&self, key: &str) -> AppResult<i32> {
        self.driver.forget(key).await
    }

    pub async fn get_or_put<Val, Fun, Fut>(&self, key: &str, setter: Fun) -> AppResult<Val>
    where
        Val: Serialize + DeserializeOwned + Clone + Sync + Send,
        Fun: FnOnce() -> Fut + Send,
        Fut: Future<Output = AppResult<Val>> + Send,
    {
        // Note: Using `Cache` here to provide context inside setter
        self.driver.get_or_put(key, setter).await
    }
}
