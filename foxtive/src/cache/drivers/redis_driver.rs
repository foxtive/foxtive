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
}
