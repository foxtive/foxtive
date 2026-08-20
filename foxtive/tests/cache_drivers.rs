//! Integration tests for cache drivers.
//!
//! These tests exercise the filesystem cache driver end-to-end
//! without requiring external services (Redis, etc.).

#[cfg(feature = "cache-filesystem")]
mod filesystem_cache_tests {
    use foxtive::cache::Cache;
    use foxtive::cache::drivers::FilesystemCacheDriver;
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;
    use tempfile::TempDir;

    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
    struct TestUser {
        name: String,
        age: u32,
    }

    fn make_cache(dir: &TempDir) -> Cache {
        let driver = Arc::new(FilesystemCacheDriver::new(dir.path().to_str().unwrap()));
        Cache::new(driver)
    }

    #[tokio::test]
    async fn put_and_get_round_trip() {
        let dir = TempDir::new().unwrap();
        let cache = make_cache(&dir);

        let user = TestUser {
            name: "Alice".into(),
            age: 30,
        };
        cache.put("user:1", &user).await.unwrap();

        let retrieved: Option<TestUser> = cache.get("user:1").await.unwrap();
        assert_eq!(retrieved, Some(user));
    }

    #[tokio::test]
    async fn get_missing_key_returns_none() {
        let dir = TempDir::new().unwrap();
        let cache = make_cache(&dir);

        let result: Option<TestUser> = cache.get("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn forget_removes_key() {
        let dir = TempDir::new().unwrap();
        let cache = make_cache(&dir);

        cache.put("key1", &"value1").await.unwrap();
        let removed = cache.forget("key1").await.unwrap();
        assert!(removed >= 0); // driver-specific count

        let result: Option<String> = cache.get("key1").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_or_put_computes_on_miss() {
        let dir = TempDir::new().unwrap();
        let cache = make_cache(&dir);

        let value: i32 = cache
            .get_or_put("computed", || async { Ok(42) })
            .await
            .unwrap();
        assert_eq!(value, 42);

        // Second call should return cached value
        let cached: i32 = cache
            .get_or_put("computed", || async { Ok(99) })
            .await
            .unwrap();
        assert_eq!(cached, 42);
    }

    #[tokio::test]
    async fn keys_returns_all_stored_keys() {
        let dir = TempDir::new().unwrap();
        let cache = make_cache(&dir);

        cache.put("a", &1).await.unwrap();
        cache.put("b", &2).await.unwrap();
        cache.put("c", &3).await.unwrap();

        let keys = cache.keys().await.unwrap();
        assert!(keys.len() >= 3);
    }

    #[tokio::test]
    async fn overwrite_existing_value() {
        let dir = TempDir::new().unwrap();
        let cache = make_cache(&dir);

        cache.put("key", &"old").await.unwrap();
        cache.put("key", &"new").await.unwrap();

        let result: Option<String> = cache.get("key").await.unwrap();
        assert_eq!(result, Some("new".to_string()));
    }
}

#[cfg(feature = "cache-in-memory")]
mod in_memory_cache_tests {
    use foxtive::cache::Cache;
    use foxtive::cache::drivers::InMemoryDriver;
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;

    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
    struct TestItem {
        id: u64,
        label: String,
    }

    fn make_cache() -> Cache {
        let driver = Arc::new(InMemoryDriver::new());
        Cache::new(driver)
    }

    #[tokio::test]
    async fn put_and_get_round_trip() {
        let cache = make_cache();
        let item = TestItem {
            id: 1,
            label: "first".into(),
        };
        cache.put("item:1", &item).await.unwrap();

        let retrieved: Option<TestItem> = cache.get("item:1").await.unwrap();
        assert_eq!(retrieved, Some(item));
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let cache = make_cache();
        let result: Option<String> = cache.get("missing").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn forget_removes_entry() {
        let cache = make_cache();
        cache.put("x", &100).await.unwrap();
        cache.forget("x").await.unwrap();

        let result: Option<i32> = cache.get("x").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_or_put_computes_on_miss() {
        let cache = make_cache();

        let val: String = cache
            .get_or_put("lazy", || async { Ok("computed".to_string()) })
            .await
            .unwrap();
        assert_eq!(val, "computed");

        // Should return cached value, not recompute
        let cached: String = cache
            .get_or_put("lazy", || async { Ok("different".to_string()) })
            .await
            .unwrap();
        assert_eq!(cached, "computed");
    }
}
