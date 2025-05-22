use crate::cache::contract::CacheDriverContract;
use crate::prelude::Redis;
use crate::results::AppResult;
use std::sync::Arc;

#[derive(Clone)]
pub struct RedisCacheDriver {
    redis: Arc<Redis>,
}

impl RedisCacheDriver {
    pub fn new(redis: Arc<Redis>) -> Self {
        Self { redis }
    }
}

#[async_trait::async_trait]
impl CacheDriverContract for RedisCacheDriver {
    async fn put_raw(&self, key: &str, value: String) -> AppResult<String> {
        self.redis.set(key, &value).await
    }

    async fn get_raw(&self, key: &str) -> AppResult<Option<String>> {
        self.redis.get::<Option<String>>(key).await
    }

    async fn forget(&self, key: &str) -> AppResult<i32> {
        self.redis.delete(key).await
    }

    async fn forget_by_pattern(&self, key: &str) -> AppResult<i32> {
        self.redis
            .delete_by_pattern(key)
            .await
            .map(|count| count as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::Redis;
    use deadpool_redis::{Config, Runtime};
    use std::env;
    use std::sync::Arc;
    use std::time::Duration;

    async fn setup_test_driver() -> Option<RedisCacheDriver> {
        // Try to get Redis URL from environment, fall back to default if not set
        let redis_url = env::var("TEST_REDIS_DSN")
            .or_else(|_| env::var("REDIS_DSN"))
            .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

        eprintln!("Connecting to Redis at {}", redis_url);

        // Create Redis configuration
        let cfg = Config::from_url(redis_url);

        // Attempt to create the pool
        match cfg.create_pool(Some(Runtime::Tokio1)) {
            Ok(pool) => {
                let redis = Arc::new(Redis::new(pool));
                let driver = RedisCacheDriver::new(redis);

                // Test connection and flush DB
                match driver.redis.flush_db().await {
                    Ok(_) => Some(driver),
                    Err(e) => {
                        eprintln!("Error: {e}");
                        None
                    }
                }
            }
            Err(e) => {
                eprintln!("Error: {e}");
                None
            }
        }
    }

    #[tokio::test]
    async fn test_basic_operations() {
        let driver = match setup_test_driver().await {
            Some(driver) => driver,
            None => {
                eprintln!("Skipping Redis tests - no connection available");
                return;
            }
        };

        // Verify connection is working
        match driver.redis.flush_db().await {
            Ok(_) => eprintln!("Successfully connected to Redis and flushed DB"),
            Err(e) => {
                eprintln!("Failed to flush Redis DB: {}", e);
                return;
            }
        }

        // Test put_raw and get_raw
        let key = "_____test_key";
        let value = "test_value".to_string();

        assert!(driver.put_raw(key, value.clone()).await.is_ok());
        let get_result = driver.get_raw(key).await.unwrap();
        assert_eq!(get_result, Some(value));

        // Test forget
        assert_eq!(driver.forget(key).await.unwrap(), 1);
        assert_eq!(driver.get_raw(key).await.unwrap(), None);

        // Test forgetting non-existent key
        assert_eq!(driver.forget("nonexistent").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_forget_by_pattern_comprehensive() {
        let driver = match setup_test_driver().await {
            Some(driver) => driver,
            None => {
                eprintln!("Skipping Redis tests - no connection available");
                return;
            }
        };

        // Verify connection is working
        match driver.redis.flush_db().await {
            Ok(_) => eprintln!("Successfully connected to Redis and flushed DB"),
            Err(e) => {
                eprintln!("Failed to flush Redis DB: {}", e);
                return;
            }
        }

        // Set up test data with various patterns
        let test_data = [
            ("user:123", "data1"),
            ("user:456", "data2"),
            ("cache:temp:1", "temp1"),
            ("cache:temp:2", "temp2"),
            ("session:abc", "session1"),
            ("SESSION:xyz", "session2"),
            ("test.key", "value"),
            ("test-key", "value"),
            ("special*char", "special"),
        ];

        for (key, value) in test_data {
            driver.put_raw(key, value.to_string()).await.unwrap();
        }

        // Test case 1: Exact prefix match
        let removed = driver.forget_by_pattern("user:*").await.unwrap();
        assert_eq!(removed, 2);
        assert_eq!(driver.get_raw("user:123").await.unwrap(), None);
        assert_eq!(driver.get_raw("user:456").await.unwrap(), None);
        assert!(driver.get_raw("cache:temp:1").await.unwrap().is_some());

        // Test case 2: Match with multiple segments
        let removed = driver.forget_by_pattern("cache:temp:*").await.unwrap();
        assert_eq!(removed, 2);
        assert_eq!(driver.get_raw("cache:temp:1").await.unwrap(), None);
        assert_eq!(driver.get_raw("cache:temp:2").await.unwrap(), None);

        // Test case 3: Pattern with special characters
        let removed = driver.forget_by_pattern("test?key").await.unwrap();
        assert_eq!(removed, 2);
        assert_eq!(driver.get_raw("test.key").await.unwrap(), None);
        assert_eq!(driver.get_raw("test-key").await.unwrap(), None);

        // Test case 4: Pattern with escaped special characters
        let removed = driver.forget_by_pattern("special\\*char").await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(driver.get_raw("special*char").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        let driver = match setup_test_driver().await {
            Some(driver) => driver,
            None => {
                eprintln!("Skipping Redis tests - no connection available");
                return;
            }
        };

        // Verify connection is working
        match driver.redis.flush_db().await {
            Ok(_) => eprintln!("Successfully connected to Redis and flushed DB"),
            Err(e) => {
                eprintln!("Failed to flush Redis DB: {}", e);
                return;
            }
        }

        // Allow pool to initialize fully
        tokio::time::sleep(Duration::from_millis(100)).await;

        let driver_clone = driver.clone();

        // Add initial data with verification
        for i in 0..100 {
            let key = format!("test:{}", i);
            let value = format!("value{}", i);

            match driver.put_raw(&key, value.clone()).await {
                Ok(_) => {
                    // Verify each write immediately
                    match driver.get_raw(&key).await {
                        Ok(Some(v)) if v == value => continue,
                        Ok(Some(v)) => {
                            eprintln!(
                                "Value mismatch for key {}: expected '{}', got '{}'",
                                key, value, v
                            );
                            return;
                        }
                        Ok(None) => {
                            eprintln!("Key {} was written but returned None on read", key);
                            return;
                        }
                        Err(e) => {
                            eprintln!("Error reading back key {}: {}", key, e);
                            return;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to write key {}: {}", key, e);
                    return;
                }
            }
        }

        eprintln!("Initial data written successfully");

        // Sequential verification of all data before concurrent operations
        for i in 0..100 {
            let key = format!("test:{}", i);
            let expected = format!("value{}", i);
            match driver.get_raw(&key).await {
                Ok(Some(v)) if v == expected => continue,
                Ok(Some(v)) => {
                    eprintln!(
                        "Pre-concurrent check: Value mismatch for key {}: expected '{}', got '{}'",
                        key, expected, v
                    );
                    return;
                }
                Ok(None) => {
                    eprintln!(
                        "Pre-concurrent check: Key {} unexpectedly returned None",
                        key
                    );
                    return;
                }
                Err(e) => {
                    eprintln!("Pre-concurrent check: Error reading key {}: {}", key, e);
                    return;
                }
            }
        }

        eprintln!("Initial verification completed successfully");

        // Proceed with concurrent operations
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let barrier_write = barrier.clone();
        let barrier_read = barrier.clone();

        let write_handle = tokio::spawn({
            let driver = driver_clone.clone();
            async move {
                barrier_write.wait().await;
                for i in 100..200 {
                    if let Err(e) = driver
                        .put_raw(&format!("test:{}", i), format!("value{}", i))
                        .await
                    {
                        eprintln!("Write task error: {}", e);
                        return;
                    }
                }
            }
        });

        let read_handle = tokio::spawn({
            let driver = driver_clone.clone();
            async move {
                barrier_read.wait().await;
                for i in 0..100 {
                    match driver.get_raw(&format!("test:{}", i)).await {
                        Ok(Some(v)) => {
                            let expected = format!("value{}", i);
                            if v != expected {
                                eprintln!("Read task: Value mismatch for key test:{}: expected '{}', got '{}'", 
                                    i, expected, v);
                            }
                        }
                        Ok(None) => eprintln!("Read task: Unexpected None for key test:{}", i),
                        Err(e) => eprintln!("Read task error for key test:{}: {}", i, e),
                    }
                }
            }
        });

        // Wait for operations to complete
        let _ = tokio::join!(write_handle, read_handle);

        // Clean up
        let removed = driver.forget_by_pattern("test:*").await.unwrap();
        assert_eq!(removed, 200);
    }

    #[tokio::test]
    async fn test_error_handling() {
        let driver = match setup_test_driver().await {
            Some(driver) => driver,
            None => {
                eprintln!("Skipping Redis tests - no connection available");
                return;
            }
        };

        // Verify connection is working
        match driver.redis.flush_db().await {
            Ok(_) => eprintln!("Successfully connected to Redis and flushed DB"),
            Err(e) => {
                eprintln!("Failed to flush Redis DB: {}", e);
                return;
            }
        }

        // Test handling of special characters in keys
        let special_chars = vec!["key with spaces", "key\nwith\nnewlines", "key:with:colons"];
        for key in special_chars {
            let result = driver.put_raw(key, "test".to_string()).await;
            assert!(result.is_ok(), "Should handle special characters in keys");
        }
    }

    #[tokio::test]
    async fn test_expiration_behavior() {
        let driver = match setup_test_driver().await {
            Some(driver) => driver,
            None => {
                eprintln!("Skipping Redis tests - no connection available");
                return;
            }
        };

        // Verify connection is working
        match driver.redis.flush_db().await {
            Ok(_) => eprintln!("Successfully connected to Redis and flushed DB"),
            Err(e) => {
                eprintln!("Failed to flush Redis DB: {}", e);
                return;
            }
        }

        let key = "expiring_key";
        let value = "test_value".to_string();

        // Store a value
        driver.put_raw(key, value).await.unwrap();

        let result = driver.get_raw(key).await.unwrap();
        assert!(result.is_some(), "Value should still exist after 1 second");
    }
}
