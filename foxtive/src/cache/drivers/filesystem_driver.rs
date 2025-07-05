use crate::cache::contract::CacheDriverContract;
use crate::results::AppResult;
use async_trait::async_trait;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct FilesystemCacheDriver {
    base_path: Arc<PathBuf>,
    // Cache for computed paths
    path_cache: Arc<RwLock<HashMap<String, PathBuf>>>,
}

impl FilesystemCacheDriver {
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: Arc::new(PathBuf::from(base_path.as_ref())),
            path_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn key_to_path(&self, key: &str) -> PathBuf {
        // Check path cache first
        if let Some(path) = self.path_cache.read().await.get(key) {
            return path.clone();
        }

        // Handle empty key specially to avoid empty filename
        let safe_key = if key.is_empty() {
            "empty_key".to_string()
        } else {
            key.replace([':', '/', '\\', '<', '>', '"', '|', '?', '*'], "_")
        };

        let path = self.base_path.join(format!("{safe_key}.cache"));
        self.path_cache
            .write()
            .await
            .insert(key.to_string(), path.clone());
        path
    }
}

#[async_trait]
impl CacheDriverContract for FilesystemCacheDriver {
    async fn keys(&self) -> AppResult<Vec<String>> {
        // Read from path cache first
        let path_cache = self.path_cache.read().await;
        let mut keys: Vec<String> = path_cache.keys().cloned().collect();

        // Also scan the directory for any files not yet in cache
        let mut dir = fs::read_dir(&*self.base_path).await?;
        while let Some(entry) = dir.next_entry().await? {
            if entry.file_type().await?.is_file() {
                if let Some(file_name) = entry.file_name().to_str() {
                    // Only process .cache files
                    if let Some(stripped) = file_name.strip_suffix(".cache") {
                        // Convert filename back to key
                        let original_key = if stripped == "empty_key" {
                            "".to_string()
                        } else {
                            stripped.to_string()
                        };

                        // Add to result if not already included from path cache
                        if !keys.contains(&original_key) {
                            keys.push(original_key);
                        }
                    }
                }
            }
        }

        Ok(keys)
    }

    async fn keys_by_pattern(&self, pattern: &str) -> AppResult<Vec<String>> {
        let regex = fancy_regex::Regex::new(pattern)?;
        let all_keys = self.keys().await?;

        Ok(all_keys
            .into_iter()
            .filter(|key| matches!(regex.is_match(key), Ok(true)))
            .collect())
    }

    async fn put_raw(&self, key: &str, value: String) -> AppResult<String> {
        let path = self.key_to_path(key).await;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let file = fs::File::create(&path).await?;
        let mut writer = BufWriter::new(file);
        writer.write_all(value.as_bytes()).await?;
        writer.flush().await?;

        Ok(key.to_string())
    }

    async fn get_raw(&self, key: &str) -> AppResult<Option<String>> {
        let path = self.key_to_path(key).await;

        match fs::File::open(&path).await {
            Ok(file) => {
                let mut reader = BufReader::new(file);
                let mut contents = String::with_capacity(1024); // Pre-allocate with reasonable size
                reader.read_to_string(&mut contents).await?;
                Ok(Some(contents))
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn forget(&self, key: &str) -> AppResult<i32> {
        let path = self.key_to_path(key).await;

        // Remove from path cache
        self.path_cache.write().await.remove(key);

        match fs::remove_file(&path).await {
            Ok(_) => Ok(1),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(0),
            Err(e) => Err(e.into()),
        }
    }

    async fn forget_by_pattern(&self, pattern: &str) -> AppResult<i32> {
        let regex = fancy_regex::Regex::new(pattern)?;
        let mut removed_count = 0;

        // First, collect matching keys from the path cache
        let path_cache = self.path_cache.read().await;
        let keys_to_remove: Vec<String> = path_cache
            .keys()
            .filter_map(|key| match regex.is_match(key) {
                Ok(true) => Some(key.clone()),
                _ => None,
            })
            .collect();
        drop(path_cache); // Release the read lock

        // Remove matching files and their cache entries
        for key in keys_to_remove {
            let path = self.key_to_path(&key).await;

            // Remove from path cache
            self.path_cache.write().await.remove(&key);

            // Remove the file
            match fs::remove_file(&path).await {
                Ok(_) => removed_count += 1,
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }

        Ok(removed_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup_test_cache() -> (FilesystemCacheDriver, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let driver = FilesystemCacheDriver::new(temp_dir.path());
        (driver, temp_dir)
    }

    #[tokio::test]
    async fn test_forget_by_pattern_basic() {
        let (driver, _temp_dir) = setup_test_cache().await;

        // Set up test data
        let test_data = [
            ("user:123", "data1"),
            ("user:456", "data2"),
            ("cache:temp:1", "temp1"),
        ];

        for (key, value) in test_data {
            driver.put_raw(key, value.to_string()).await.unwrap();
        }

        // Test exact prefix match
        let removed = driver.forget_by_pattern("^user:.*").await.unwrap();
        assert_eq!(removed, 2);
        assert_eq!(driver.get_raw("user:123").await.unwrap(), None);
        assert_eq!(driver.get_raw("user:456").await.unwrap(), None);
        assert!(driver.get_raw("cache:temp:1").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_forget_by_pattern_comprehensive() {
        let (driver, _temp_dir) = setup_test_cache().await;

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
            ("", "empty"),
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

        // Test case 5: Empty pattern (matches empty strings)
        let removed = driver.forget_by_pattern("^$").await.unwrap(); // Using ^$ to match only empty strings
        assert_eq!(removed, 1); // Should match only the empty key
        assert_eq!(driver.get_raw("").await.unwrap(), None);

        // Test case 6: Pattern matching empty key
        let empty_key = "";
        // First verify the empty key was stored properly
        assert!(driver.put_raw(empty_key, "empty".to_string()).await.is_ok());
        assert_eq!(
            driver.get_raw(empty_key).await.unwrap(),
            Some("empty".to_string())
        );

        // Now test the pattern match
        let removed = driver.forget_by_pattern("^$").await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(driver.get_raw(empty_key).await.unwrap(), None);

        // Test case 7: Pattern with escaped special characters
        let removed = driver.forget_by_pattern(r"special\*char").await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(driver.get_raw("special*char").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_forget_by_pattern_concurrent() {
        let (driver, _temp_dir) = setup_test_cache().await;
        let driver_clone = driver.clone();

        // Add initial data
        for i in 0..100 {
            driver
                .put_raw(&format!("test:{i}"), format!("value{i}"))
                .await
                .unwrap();
        }

        // Spawn concurrent tasks with non-overlapping patterns
        let driver_clone_1 = driver_clone.clone();
        let handle1 = tokio::spawn(async move {
            // Pattern for 0-49
            driver_clone_1
                .forget_by_pattern("^test:([0-4]\\d|[0-9])$")
                .await
                .unwrap()
        });

        let driver_clone_2 = driver_clone.clone();
        let handle2 = tokio::spawn(async move {
            // Pattern for 50-99
            driver_clone_2
                .forget_by_pattern("^test:[5-9]\\d$")
                .await
                .unwrap()
        });

        // Wait for both tasks to complete
        let (result1, result2) = tokio::join!(handle1, handle2);

        let total_removed = result1.unwrap() + result2.unwrap();
        assert_eq!(total_removed, 100, "Failed to remove all items");

        // Verify all cache entries are gone
        for i in 0..100 {
            assert!(driver_clone
                .get_raw(&format!("test:{i}"))
                .await
                .unwrap()
                .is_none());
        }
    }

    #[tokio::test]
    async fn test_forget_by_pattern_invalid_regex() {
        let (driver, _temp_dir) = setup_test_cache().await;

        // Test with invalid regex pattern
        let result = driver.forget_by_pattern("[").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_forget_by_pattern_no_matches() {
        let (driver, _temp_dir) = setup_test_cache().await;

        // Add some test data
        driver
            .put_raw("test:1", "value1".to_string())
            .await
            .unwrap();
        driver
            .put_raw("test:2", "value2".to_string())
            .await
            .unwrap();

        // Test pattern that doesn't match any keys
        let removed = driver.forget_by_pattern("^nonexistent:.*").await.unwrap();
        assert_eq!(removed, 0);

        // Verify original data still exists
        assert!(driver.get_raw("test:1").await.unwrap().is_some());
        assert!(driver.get_raw("test:2").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_keys_with_data() {
        let (driver, _temp_dir) = setup_test_cache().await;

        // Set up test data
        let test_data = [
            ("user_123", "data1"),     // Using underscore instead of colon
            ("user_456", "data2"),     // Using underscore instead of colon
            ("cache_temp_1", "temp1"), // Using underscore instead of colon
            ("", "empty"),             // Test empty key
        ];

        for (key, value) in test_data {
            driver.put_raw(key, value.to_string()).await.unwrap();
        }

        let mut keys = driver.keys().await.unwrap();
        keys.sort(); // Sort for consistent comparison

        let mut expected: Vec<String> = test_data.iter().map(|(k, _)| k.to_string()).collect();
        expected.sort();

        assert_eq!(keys, expected, "Retrieved keys should match inserted keys");
    }

    #[tokio::test]
    async fn test_keys_after_deletion() {
        let (driver, _temp_dir) = setup_test_cache().await;

        // Set up initial data
        let test_data = [("key1", "value1"), ("key2", "value2"), ("key3", "value3")];

        for (key, value) in test_data {
            driver.put_raw(key, value.to_string()).await.unwrap();
        }

        // Delete one key
        driver.forget("key2").await.unwrap();

        let mut keys = driver.keys().await.unwrap();
        keys.sort();

        let expected = vec!["key1".to_string(), "key3".to_string()];
        assert_eq!(keys, expected, "Keys should reflect deletion");
    }

    #[tokio::test]
    async fn test_keys_by_pattern_basic() {
        let (driver, _temp_dir) = setup_test_cache().await;

        // Set up test data
        let test_data = [
            ("user:123", "data1"),
            ("user:456", "data2"),
            ("cache:temp:1", "temp1"),
            ("cache:temp:2", "temp2"),
        ];

        for (key, value) in test_data {
            driver.put_raw(key, value.to_string()).await.unwrap();
        }

        // Test exact prefix match
        let mut keys = driver.keys_by_pattern("^user:.*").await.unwrap();
        keys.sort();
        assert_eq!(
            keys,
            vec!["user:123".to_string(), "user:456".to_string()],
            "Should match user: prefix"
        );

        // Test cache prefix match
        let mut keys = driver.keys_by_pattern("^cache:temp:.*").await.unwrap();
        keys.sort();
        assert_eq!(
            keys,
            vec!["cache:temp:1".to_string(), "cache:temp:2".to_string()],
            "Should match cache:temp: prefix"
        );
    }

    #[tokio::test]
    async fn test_keys_by_pattern_complex() {
        let (driver, _temp_dir) = setup_test_cache().await;

        // Set up test data with various patterns
        let test_data = [
            ("abc123", "value1"),
            ("ABC456", "value2"),
            ("test_key", "value3"),
            ("test_key2", "value4"),
            ("123test", "value5"),
        ];

        for (key, value) in test_data {
            driver.put_raw(key, value.to_string()).await.unwrap();
        }

        // Test case-insensitive pattern
        let mut keys = driver.keys_by_pattern("(?i)^abc").await.unwrap();
        // Sort case-insensitively
        keys.sort_by_key(|k| k.to_lowercase());

        let mut expected = vec!["abc123".to_string(), "ABC456".to_string()];
        expected.sort_by_key(|k| k.to_lowercase());
        assert_eq!(keys, expected, "Should match case-insensitive");

        // Test pattern with underscore
        let mut keys = driver.keys_by_pattern("test_key.*").await.unwrap();
        keys.sort();
        let mut expected = vec!["test_key".to_string(), "test_key2".to_string()];
        expected.sort();
        assert_eq!(keys, expected, "Should match keys with underscore");

        // Test numeric prefix
        let keys = driver.keys_by_pattern("^\\d+").await.unwrap();
        assert_eq!(
            keys,
            vec!["123test".to_string()],
            "Should match numeric prefix"
        );
    }

    #[tokio::test]
    async fn test_keys_by_pattern_no_matches() {
        let (driver, _temp_dir) = setup_test_cache().await;

        // Add some test data
        driver
            .put_raw("test:1", "value1".to_string())
            .await
            .unwrap();
        driver
            .put_raw("test:2", "value2".to_string())
            .await
            .unwrap();

        let keys = driver.keys_by_pattern("^nonexistent:.*").await.unwrap();
        assert!(keys.is_empty(), "Should return empty vec for no matches");
    }

    #[tokio::test]
    async fn test_keys_by_pattern_invalid_regex() {
        let (driver, _temp_dir) = setup_test_cache().await;

        // Test with invalid regex pattern
        let result = driver.keys_by_pattern("[").await;
        assert!(result.is_err(), "Should return error for invalid regex");
    }
}
