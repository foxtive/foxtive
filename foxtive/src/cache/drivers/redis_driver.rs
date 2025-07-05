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
    async fn keys(&self) -> AppResult<Vec<String>> {
        // Use Redis KEYS command to get all keys
        self.redis.keys().await
    }

    async fn keys_by_pattern(&self, pattern: &str) -> AppResult<Vec<String>> {
        // Use Redis KEYS command with the provided pattern directly
        // Redis patterns use glob-style patterns, which is different from regex
        // but the contract expects regex patterns, so we need to convert
        let redis_pattern = regex_to_redis_pattern(pattern);
        self.redis.keys_by_pattern(&redis_pattern).await
    }

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

// Helper function to convert regex patterns to Redis glob patterns
fn regex_to_redis_pattern(pattern: &str) -> String {
    // Handle some common regex patterns and convert them to Redis patterns
    let mut redis_pattern = pattern.to_string();

    // Replace regex start/end markers
    redis_pattern = redis_pattern.replace("^", "");
    redis_pattern = redis_pattern.replace("$", "");

    // Replace regex .* with Redis *
    redis_pattern = redis_pattern.replace(".*", "*");

    // Replace regex dot with Redis ?
    redis_pattern = redis_pattern.replace(".", "?");

    // Handle case-insensitive flag by removing it (Redis KEYS is case-sensitive)
    redis_pattern = redis_pattern.replace("(?i)", "");

    // Escape special Redis pattern characters that might be in the regex
    redis_pattern = redis_pattern.replace("[", "\\[");
    redis_pattern = redis_pattern.replace("]", "\\]");

    redis_pattern
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

        eprintln!("Connecting to Redis at {redis_url}");

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
                eprintln!("Failed to flush Redis DB: {e}",);
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
                eprintln!("Failed to flush Redis DB: {e}");
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
                eprintln!("Failed to flush Redis DB: {e}");
                return;
            }
        }

        // Allow pool to initialize fully
        tokio::time::sleep(Duration::from_millis(100)).await;

        let driver_clone = driver.clone();

        // Add initial data with verification
        for i in 0..100 {
            let key = format!("test:{i}");
            let value = format!("value{i}");

            match driver.put_raw(&key, value.clone()).await {
                Ok(_) => {
                    // Verify each write immediately
                    match driver.get_raw(&key).await {
                        Ok(Some(v)) if v == value => continue,
                        Ok(Some(v)) => {
                            eprintln!(
                                "Value mismatch for key {key}: expected '{value}', got '{v}'"
                            );
                            return;
                        }
                        Ok(None) => {
                            eprintln!("Key {key} was written but returned None on read");
                            return;
                        }
                        Err(e) => {
                            eprintln!("Error reading back key {key}: {e}");
                            return;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to write key {key}: {e}");
                    return;
                }
            }
        }

        eprintln!("Initial data written successfully");

        // Sequential verification of all data before concurrent operations
        for i in 0..100 {
            let key = format!("test:{i}");
            let expected = format!("value{i}");
            match driver.get_raw(&key).await {
                Ok(Some(v)) if v == expected => continue,
                Ok(Some(v)) => {
                    eprintln!(
                        "Pre-concurrent check: Value mismatch for key {key}: expected '{expected}', got '{v}'"
                    );
                    return;
                }
                Ok(None) => {
                    eprintln!("Pre-concurrent check: Key {key} unexpectedly returned None");
                    return;
                }
                Err(e) => {
                    eprintln!("Pre-concurrent check: Error reading key {key}: {e}");
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
                        .put_raw(&format!("test:{i}"), format!("value{i}"))
                        .await
                    {
                        eprintln!("Write task error: {e}");
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
                    match driver.get_raw(&format!("test:{i}")).await {
                        Ok(Some(v)) => {
                            let expected = format!("value{i}");
                            if v != expected {
                                eprintln!("Read task: Value mismatch for key test:{i}: expected '{expected}', got '{v}'");
                            }
                        }
                        Ok(None) => eprintln!("Read task: Unexpected None for key test:{i}"),
                        Err(e) => eprintln!("Read task error for key test:{i}: {e}"),
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
                eprintln!("Failed to flush Redis DB: {e}");
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
                eprintln!("Failed to flush Redis DB: {e}");
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

    #[tokio::test]
    async fn test_keys_empty_db() {
        let driver = match setup_test_driver().await {
            Some(driver) => driver,
            None => {
                eprintln!("Skipping Redis tests - no connection available");
                return;
            }
        };

        // Ensure DB is empty
        driver.redis.flush_db().await.unwrap();

        let keys = driver.keys().await.unwrap();
        assert!(keys.is_empty(), "Empty database should return no keys");
    }

    #[tokio::test]
    async fn test_keys_with_data() {
        let driver = match setup_test_driver().await {
            Some(driver) => driver,
            None => {
                eprintln!("Skipping Redis tests - no connection available");
                return;
            }
        };

        driver.redis.flush_db().await.unwrap();

        // Set up test data
        let test_data = [
            ("key1", "value1"),
            ("key2", "value2"),
            ("prefix:key3", "value3"),
            ("", "empty_value"), // Test empty key
        ];

        for (key, value) in test_data {
            driver.put_raw(key, value.to_string()).await.unwrap();
        }

        let mut keys = driver.keys().await.unwrap();
        keys.sort();

        let mut expected: Vec<String> = test_data.iter().map(|(k, _)| k.to_string()).collect();
        expected.sort();

        assert_eq!(keys, expected, "Retrieved keys should match inserted keys");
    }

    #[tokio::test]
    async fn test_keys_after_deletion() {
        let driver = match setup_test_driver().await {
            Some(driver) => driver,
            None => {
                eprintln!("Skipping Redis tests - no connection available");
                return;
            }
        };

        driver.redis.flush_db().await.unwrap();

        // Insert test data
        let test_data = [
            ("test1", "value1"),
            ("test2", "value2"),
            ("test3", "value3"),
        ];

        for (key, value) in test_data {
            driver.put_raw(key, value.to_string()).await.unwrap();
        }

        // Delete one key
        driver.forget("test2").await.unwrap();

        let mut keys = driver.keys().await.unwrap();
        keys.sort();

        let expected = vec!["test1".to_string(), "test3".to_string()];
        assert_eq!(keys, expected, "Keys should not include deleted key");
    }

    #[tokio::test]
    async fn test_keys_by_pattern_basic() {
        let driver = match setup_test_driver().await {
            Some(driver) => driver,
            None => {
                eprintln!("Skipping Redis tests - no connection available");
                return;
            }
        };

        driver.redis.flush_db().await.unwrap();

        // Set up test data with different patterns
        let test_data = [
            ("user:1", "value1"),
            ("user:2", "value2"),
            ("profile:1", "value3"),
            ("other", "value4"),
        ];

        for (key, value) in test_data {
            driver.put_raw(key, value.to_string()).await.unwrap();
        }

        // Test prefix match (regex pattern will be converted to Redis pattern)
        let mut keys = driver.keys_by_pattern("^user:.*").await.unwrap();
        keys.sort();
        assert_eq!(
            keys,
            vec!["user:1".to_string(), "user:2".to_string()],
            "Should match user: prefix"
        );

        // Test exact match
        let keys = driver.keys_by_pattern("^other$").await.unwrap();
        assert_eq!(
            keys,
            vec!["other".to_string()],
            "Should match exact pattern"
        );
    }

    #[tokio::test]
    async fn test_keys_by_pattern_wildcards() {
        let driver = match setup_test_driver().await {
            Some(driver) => driver,
            None => {
                eprintln!("Skipping Redis tests - no connection available");
                return;
            }
        };

        driver.redis.flush_db().await.unwrap();

        // Set up test data for wildcard matching
        let test_data = [
            ("test1", "value1"),
            ("test2", "value2"),
            ("test11", "value3"),
            ("atest1", "value4"),
        ];

        for (key, value) in test_data {
            driver.put_raw(key, value.to_string()).await.unwrap();
        }

        // Test single character wildcard (. in regex becomes ? in Redis)
        let mut keys = driver.keys_by_pattern("test.").await.unwrap();
        keys.sort();
        assert_eq!(
            keys,
            vec!["test1".to_string(), "test2".to_string()],
            "Should match single character wildcard"
        );

        // Test multi-character wildcard (.* in regex becomes * in Redis)
        let mut keys = driver.keys_by_pattern("test.*").await.unwrap();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "test1".to_string(),
                "test11".to_string(),
                "test2".to_string()
            ],
            "Should match multi-character wildcard"
        );
    }

    #[tokio::test]
    async fn test_keys_by_pattern_special_chars() {
        let driver = match setup_test_driver().await {
            Some(driver) => driver,
            None => {
                eprintln!("Skipping Redis tests - no connection available");
                return;
            }
        };

        driver.redis.flush_db().await.unwrap();

        // Set up test data with special characters
        let test_data = [
            ("test[1]", "value1"),
            ("test[2]", "value2"),
            ("test{3}", "value3"),
        ];

        for (key, value) in test_data {
            driver.put_raw(key, value.to_string()).await.unwrap();
        }

        // Test pattern with escaped special characters
        let mut keys = driver.keys_by_pattern("test\\[.*\\]").await.unwrap();
        keys.sort();
        assert_eq!(
            keys,
            vec!["test[1]".to_string(), "test[2]".to_string()],
            "Should match escaped special characters"
        );
    }

    #[tokio::test]
    async fn test_keys_by_pattern_empty() {
        let driver = match setup_test_driver().await {
            Some(driver) => driver,
            None => {
                eprintln!("Skipping Redis tests - no connection available");
                return;
            }
        };

        driver.redis.flush_db().await.unwrap();

        // Add some test data
        driver.put_raw("test1", "value1".to_string()).await.unwrap();
        driver.put_raw("test2", "value2".to_string()).await.unwrap();

        let keys = driver.keys_by_pattern("").await.unwrap();
        assert!(keys.is_empty(), "Empty pattern should return no matches");
    }
}
