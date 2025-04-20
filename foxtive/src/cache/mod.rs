//! # Cache Module
//!
//! A flexible caching system that provides a high-level interface for storing and retrieving
//! serializable data using different storage backends.
//!
//! ## Features
//!
//! This module requires at least one of the following features to be enabled:
//! - `cache-redis`
//! - `cache-filesystem`
//!
//! ## Example
//!
//! ```no_run
//! use std::sync::Arc;
//! use foxtive::cache::{Cache, drivers::FilesystemCacheDriver};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Serialize, Deserialize)]
//! struct MySerializableStruct {}
//!
//! #[tokio::main]
//! async fn main() {
//!     let driver = Arc::new(FilesystemCacheDriver::new("./"));
//!     let cache = Cache::new(driver);
//!
//!     let value = MySerializableStruct {};
//!     cache.put("my-key", &value).await.unwrap();
//!
//!     let retrieved: Option<MySerializableStruct> = cache.get("my-key").await.unwrap();
//! }
//! ```

pub mod contract;
pub mod drivers;

use crate::cache::contract::{CacheDriverContract, CacheDriverExt};
use crate::prelude::AppResult;
use serde::{de::DeserializeOwned, Serialize};
use std::future::Future;
use std::sync::Arc;

/// A generic caching interface that provides methods for storing and retrieving serialized data.
///
/// The `Cache` struct acts as a wrapper around different cache driver implementations,
/// providing a consistent interface regardless of the underlying storage mechanism.
#[derive(Clone)]
pub struct Cache {
    driver: Arc<dyn CacheDriverContract>,
}

impl Cache {
    /// Creates a new `Cache` instance with the specified driver.
    ///
    /// # Arguments
    ///
    /// * `driver` - An implementation of `CacheDriverContract` wrapped in an `Arc`
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use foxtive::cache::{Cache, drivers::FilesystemCacheDriver};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let driver = Arc::new(FilesystemCacheDriver::new("./"));
    ///     let cache = Cache::new(driver);
    /// }
    /// ```
    pub fn new(driver: Arc<dyn CacheDriverContract>) -> Self {
        Self { driver }
    }

    /// Returns a clone of the underlying driver.
    ///
    /// This method is useful when you need direct access to the driver implementation.
    pub fn driver(&self) -> Arc<dyn CacheDriverContract> {
        Arc::clone(&self.driver)
    }

    /// Stores a serializable value in the cache under the specified key.
    ///
    /// # Arguments
    ///
    /// * `key` - The key under which to store the value
    /// * `value` - The value to store, which must implement `Serialize`
    ///
    /// # Returns
    ///
    /// Returns `AppResult<String>` containing the cached key on success
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use foxtive::cache::{Cache, drivers::FilesystemCacheDriver};
    ///
    /// use serde::{Serialize, Deserialize};
    ///
    /// #[derive(Serialize, Deserialize)]
    /// struct User {
    ///     name: String
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let driver = Arc::new(FilesystemCacheDriver::new("./"));
    ///     let cache = Cache::new(driver);
    ///
    ///     let user = User { name: "John".to_string() };
    ///     cache.put("user:1", &user).await.unwrap();
    /// }
    /// ```
    pub async fn put<T>(&self, key: &str, value: &T) -> AppResult<String>
    where
        T: Serialize + Sync,
    {
        self.driver.put_raw(key, serde_json::to_string(value)?).await
    }

    /// Retrieves a value from the cache and deserializes it into the specified type.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to look up
    ///
    /// # Returns
    ///
    /// Returns `AppResult<Option<T>>` where:
    /// - `Ok(Some(T))` - Value was found and successfully deserialized
    /// - `Ok(None)` - Key was not found in cache
    /// - `Err(_)` - An error occurred during retrieval or deserialization
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use foxtive::cache::{Cache, drivers::FilesystemCacheDriver};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let driver = Arc::new(FilesystemCacheDriver::new("./"));
    ///     let cache = Cache::new(driver);
    ///
    ///     let user: Option<String> = cache.get("user:1:name").await.unwrap();
    /// }
    /// ```
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

    /// Removes a value from the cache.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to remove
    ///
    /// # Returns
    ///
    /// Returns `AppResult<i32>` indicating the number of keys that were removed
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use foxtive::cache::{Cache, drivers::FilesystemCacheDriver};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let driver = Arc::new(FilesystemCacheDriver::new("./"));
    ///     let cache = Cache::new(driver);
    ///
    ///     let removed = cache.forget("user:1").await.unwrap();
    ///     assert_eq!(removed, 1);
    /// }
    /// ```
    pub async fn forget(&self, key: &str) -> AppResult<i32> {
        self.driver.forget(key).await
    }

    /// Retrieves a value from the cache or computes and stores it if not present.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to look up
    /// * `setter` - A closure that computes the value if not found in cache
    ///
    /// # Returns
    ///
    /// Returns `AppResult<Val>` containing either the cached value or the newly computed value
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use foxtive::cache::{Cache, drivers::FilesystemCacheDriver};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let driver = Arc::new(FilesystemCacheDriver::new("./"));
    ///     let cache = Cache::new(driver);
    /// 
    ///     let user = cache.get_or_put("user:1", || async {
    ///         // Expensive operation to fetch user from database
    ///         Ok(1)
    ///     }).await.unwrap();
    /// }
    /// ```
    pub async fn get_or_put<Val, Fun, Fut>(&self, key: &str, setter: Fun) -> AppResult<Val>
    where
        Val: Serialize + DeserializeOwned + Clone + Sync + Send,
        Fun: FnOnce() -> Fut + Send,
        Fut: Future<Output = AppResult<Val>> + Send,
    {
        self.driver.get_or_put(key, setter).await
    }
}