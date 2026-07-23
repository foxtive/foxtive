use crate::cache::contract::CacheDriverContract;
use crate::results::AppResult;
use sha2::{Digest, Sha256};
use std::future::Future;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};

#[derive(Clone)]
pub struct FilesystemCacheDriver {
    base_path: Arc<PathBuf>,
    /// Optional TTL for cache entries. Entries older than this are treated as expired.
    default_ttl: Option<Duration>,
}

impl FilesystemCacheDriver {
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: Arc::new(PathBuf::from(base_path.as_ref())),
            default_ttl: None,
        }
    }

    /// Set the default TTL for all cache entries.
    ///
    /// Entries older than the TTL are treated as expired and return `None` on `get_raw()`.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = Some(ttl);
        self
    }

    fn key_to_path(&self, key: &str) -> PathBuf {
        // Use SHA-256 hash of the key to prevent collisions and ensure safe filenames
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let hash = hex::encode(hasher.finalize());
        self.base_path.join(format!("{hash}.cache"))
    }

    /// Write key + value to a file. Format: `[4-byte key_len][key bytes][value bytes]`.
    /// This allows `keys()` to recover original keys from hashed filenames.
    async fn write_entry(path: &Path, key: &str, value: &str) -> std::io::Result<()> {
        let key_bytes = key.as_bytes();
        let key_len = (key_bytes.len() as u32).to_be_bytes();

        let temp_path = path.with_extension("cache.tmp");
        let file = fs::File::create(&temp_path).await?;
        let mut writer = BufWriter::new(file);
        writer.write_all(&key_len).await?;
        writer.write_all(key_bytes).await?;
        writer.write_all(value.as_bytes()).await?;
        writer.flush().await?;
        drop(writer);

        fs::rename(&temp_path, path).await?;
        Ok(())
    }

    /// Read the original key from a cache file. Returns `None` if the file
    /// is malformed or unreadable.
    async fn read_entry_key(path: &Path) -> Option<String> {
        let file = fs::File::open(path).await.ok()?;
        let mut reader = BufReader::new(file);

        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await.ok()?;
        let key_len = u32::from_be_bytes(len_buf) as usize;

        // Sanity check: key shouldn't be absurdly long
        if key_len > 10_000 {
            return None;
        }

        let mut key_bytes = vec![0u8; key_len];
        reader.read_exact(&mut key_bytes).await.ok()?;

        String::from_utf8(key_bytes).ok()
    }
}

impl CacheDriverContract for FilesystemCacheDriver {
    fn keys(&self) -> Pin<Box<dyn Future<Output = AppResult<Vec<String>>> + Send + '_>> {
        Box::pin(async move {
            let mut keys: Vec<String> = Vec::new();

            // Scan the directory for .cache files and recover original keys
            // from the file content (since filenames are SHA-256 hashes).
            let mut dir = fs::read_dir(&*self.base_path).await?;
            while let Some(entry) = dir.next_entry().await? {
                if entry.file_type().await?.is_file()
                    && let Some(file_name) = entry.file_name().to_str()
                    && file_name.ends_with(".cache")
                    && let Some(key) = Self::read_entry_key(&entry.path()).await
                {
                    keys.push(key);
                }
            }

            Ok(keys)
        })
    }

    /// Scans the cache directory for files matching the given regex pattern.
    ///
    /// # Performance Note
    ///
    /// This operation performs a full directory scan and compiles the regex on each call.
    /// For caches with many keys, this can be expensive. Consider using more specific patterns
    /// or implementing a key index if pattern matching is frequently used.
    fn keys_by_pattern(
        &self,
        pattern: &str,
    ) -> Pin<Box<dyn Future<Output = AppResult<Vec<String>>> + Send + '_>> {
        let pattern = pattern.to_string();
        Box::pin(async move {
            let regex = fancy_regex::Regex::new(&pattern)?;
            let all_keys = self.keys().await?;

            Ok(all_keys
                .into_iter()
                .filter(|key| matches!(regex.is_match(key), Ok(true)))
                .collect())
        })
    }

    fn put_raw(
        &self,
        key: &str,
        value: String,
    ) -> Pin<Box<dyn Future<Output = AppResult<String>> + Send + '_>> {
        let key = key.to_string();
        Box::pin(async move {
            let path = self.key_to_path(&key);

            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).await?;
            }

            // Store key + value so keys() can recover original keys from hashed filenames
            Self::write_entry(&path, &key, &value).await?;

            Ok(key)
        })
    }

    fn get_raw(
        &self,
        key: &str,
    ) -> Pin<Box<dyn Future<Output = AppResult<Option<String>>> + Send + '_>> {
        let key = key.to_string();
        let ttl = self.default_ttl;
        Box::pin(async move {
            let path = self.key_to_path(&key);

            let file = match fs::File::open(&path).await {
                Ok(f) => f,
                Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e.into()),
            };

            // Check TTL expiration
            if let Some(ttl) = ttl
                && let Ok(metadata) = file.metadata().await
                && let Ok(modified) = metadata.modified()
            {
                let age = SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or(Duration::MAX);
                if age > ttl {
                    // Entry expired - remove it asynchronously
                    drop(file);
                    let _ = fs::remove_file(&path).await;
                    return Ok(None);
                }
            }

            let mut reader = BufReader::new(file);

            // Skip the key header: [4-byte key_len][key bytes]
            let mut len_buf = [0u8; 4];
            reader.read_exact(&mut len_buf).await?;
            let key_len = u32::from_be_bytes(len_buf) as usize;
            // Skip key bytes
            let mut skip_buf = vec![0u8; key_len];
            reader.read_exact(&mut skip_buf).await?;

            // Read the value
            let mut contents = String::with_capacity(1024);
            reader.read_to_string(&mut contents).await?;
            Ok(Some(contents))
        })
    }

    fn forget(&self, key: &str) -> Pin<Box<dyn Future<Output = AppResult<i32>> + Send + '_>> {
        let key = key.to_string();
        Box::pin(async move {
            let path = self.key_to_path(&key);

            match fs::remove_file(&path).await {
                Ok(_) => Ok(1),
                Err(e) if e.kind() == ErrorKind::NotFound => Ok(0),
                Err(e) => Err(e.into()),
            }
        })
    }

    /// Removes all cache entries matching the given regex pattern.
    ///
    /// # Performance Note
    ///
    /// This operation performs a full directory scan and compiles the regex on each call.
    /// For caches with many keys, this can be expensive. Consider using more specific patterns
    /// or implementing a key index if pattern matching is frequently used.
    fn forget_by_pattern(
        &self,
        pattern: &str,
    ) -> Pin<Box<dyn Future<Output = AppResult<i32>> + Send + '_>> {
        let pattern = pattern.to_string();
        Box::pin(async move {
            let regex = fancy_regex::Regex::new(&pattern)?;
            let mut removed_count = 0;

            // Collect all keys from directory scan
            let all_keys = self.keys().await?;
            let keys_to_remove: Vec<String> = all_keys
                .into_iter()
                .filter(|key| matches!(regex.is_match(key), Ok(true)))
                .collect();

            // Remove matching files
            for key in keys_to_remove {
                let path = self.key_to_path(&key);

                // Remove the file
                match fs::remove_file(&path).await {
                    Ok(_) => removed_count += 1,
                    Err(e) if e.kind() == ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
            }

            Ok(removed_count)
        })
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
            assert!(
                driver_clone
                    .get_raw(&format!("test:{i}"))
                    .await
                    .unwrap()
                    .is_none()
            );
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
