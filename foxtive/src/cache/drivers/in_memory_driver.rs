use crate::cache::contract::CacheDriverContract;
use crate::results::AppResult;
use dashmap::DashMap;
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct InMemoryDriver {
    storage: Arc<DashMap<String, String>>,
}

impl InMemoryDriver {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl CacheDriverContract for InMemoryDriver {
    async fn put_raw(&self, key: &str, value: String) -> AppResult<String> {
        self.storage.insert(key.to_string(), value.clone());
        Ok(value)
    }

    async fn get_raw(&self, key: &str) -> AppResult<Option<String>> {
        Ok(self.storage.get(key).map(|value| value.value().clone()))
    }

    async fn forget(&self, key: &str) -> AppResult<i32> {
        Ok(if self.storage.remove(key).is_some() {
            1
        } else {
            0
        })
    }

    async fn forget_by_pattern(&self, pattern: &str) -> AppResult<i32> {
        let regex = fancy_regex::Regex::new(pattern)?;
        let mut removed_count = 0;

        // Collect keys to remove to avoid mutation during iteration
        let keys_to_remove: Vec<String> = self
            .storage
            .iter()
            .filter_map(|entry| {
                let key = entry.key();
                match regex.is_match(key) {
                    Ok(true) => Some(key.clone()),
                    _ => None,
                }
            })
            .collect();

        // Remove the matched keys
        for key in keys_to_remove {
            if self.storage.remove(&key).is_some() {
                removed_count += 1;
            }
        }

        Ok(removed_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_driver() {
        let driver = InMemoryDriver::new();

        // Test put and get
        let key = "test_key";
        let value = "test_value".to_string();

        assert!(driver.put_raw(key, value.clone()).await.is_ok());
        let get_result = driver.get_raw(key).await.unwrap();
        assert_eq!(get_result, Some(value));

        // Test forget
        assert_eq!(driver.forget(key).await.unwrap(), 1);
        assert_eq!(driver.get_raw(key).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_forget_by_pattern_comprehensive() {
        let driver = InMemoryDriver::new();

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
            ("", "empty"), // Empty key
            ("special*char", "special"),
        ];

        for (key, value) in test_data {
            driver.put_raw(key, value.to_string()).await.unwrap();
        }

        // Test case 1: Exact prefix match
        let removed = driver.forget_by_pattern("^user:.*").await.unwrap();
        assert_eq!(removed, 2);
        assert_eq!(driver.get_raw("user:123").await.unwrap(), None);
        assert_eq!(driver.get_raw("user:456").await.unwrap(), None);
        assert!(driver.get_raw("cache:temp:1").await.unwrap().is_some());

        // Test case 2: Match with multiple segments
        let removed = driver.forget_by_pattern("^cache:temp:.*").await.unwrap();
        assert_eq!(removed, 2);
        assert_eq!(driver.get_raw("cache:temp:1").await.unwrap(), None);
        assert_eq!(driver.get_raw("cache:temp:2").await.unwrap(), None);

        // Test case 3: Case-insensitive match
        let removed = driver.forget_by_pattern("(?i)^session:.*").await.unwrap();
        assert_eq!(removed, 2);
        assert_eq!(driver.get_raw("session:abc").await.unwrap(), None);
        assert_eq!(driver.get_raw("SESSION:xyz").await.unwrap(), None);

        // Test case 4: Pattern with special characters
        let removed = driver.forget_by_pattern("test[.-]key").await.unwrap();
        assert_eq!(removed, 2);
        assert_eq!(driver.get_raw("test.key").await.unwrap(), None);
        assert_eq!(driver.get_raw("test-key").await.unwrap(), None);

        // Test case 5: Empty pattern (should match everything)
        let driver_empty = InMemoryDriver::new();
        driver_empty
            .put_raw("key1", "value1".to_string())
            .await
            .unwrap();
        driver_empty
            .put_raw("key2", "value2".to_string())
            .await
            .unwrap();
        let removed = driver_empty.forget_by_pattern(".*").await.unwrap();
        assert_eq!(removed, 2);

        // Test case 6: Pattern matching empty key
        let removed = driver.forget_by_pattern("^$").await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(driver.get_raw("").await.unwrap(), None);

        // Test case 7: Pattern with escaped special characters
        let removed = driver.forget_by_pattern(r"special\*char").await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(driver.get_raw("special*char").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_forget_by_pattern_concurrent() {
        use tokio;

        let driver = InMemoryDriver::new();
        let driver_clone = driver.clone();

        // Add initial data
        for i in 0..100 {
            driver
                .put_raw(&format!("test:{}", i), format!("value{}", i))
                .await
                .unwrap();
        }

        // Spawn concurrent tasks with non-overlapping patterns that cover all numbers 0-99
        let handle1 = tokio::spawn(async move {
            // Pattern for 0-49: matches both single and double digits
            driver
                .forget_by_pattern("^test:([0-4]\\d|[0-9])$")
                .await
                .unwrap()
        });

        let driver_clone_2 = driver_clone.clone();
        let handle2 = tokio::spawn(async move {
            // Pattern for 50-99: matches both single and double digits
            driver_clone_2
                .forget_by_pattern("^test:[5-9]\\d$")
                .await
                .unwrap()
        });

        // Wait for both tasks to complete
        let (result1, result2) = tokio::join!(handle1, handle2);

        let total_removed = result1.unwrap() + result2.unwrap();
        assert_eq!(
            total_removed, 100,
            "Failed to remove all items. Only removed {}",
            total_removed
        );

        // Verify all keys are gone
        let remaining = driver_clone.storage.iter().count();
        assert_eq!(remaining, 0, "Some keys remained in storage: {}", remaining);
    }
}
