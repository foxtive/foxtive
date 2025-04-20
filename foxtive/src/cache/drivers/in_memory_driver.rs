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
}
