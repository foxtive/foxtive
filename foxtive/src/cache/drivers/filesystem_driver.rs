use crate::cache::contract::CacheDriverContract;
use crate::results::AppResult;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone)]
pub struct FilesystemCacheDriver {
    base_path: Arc<PathBuf>,
}

impl FilesystemCacheDriver {
    pub fn new<P: Into<PathBuf>>(base_path: P) -> Self {
        Self {
            base_path: Arc::new(base_path.into()),
        }
    }

    fn key_to_path(&self, key: &str) -> PathBuf {
        let safe_key = key.replace(":", "_");
        self.base_path.join(format!("{}.cache", safe_key))
    }
}

#[async_trait]
impl CacheDriverContract for FilesystemCacheDriver {
    async fn put_raw(&self, key: &str, value: String) -> AppResult<String> {
        let path = self.key_to_path(key);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut file = fs::File::create(&path).await?;
        file.write_all(value.as_bytes()).await?;

        Ok(key.to_string())
    }

    async fn get_raw(&self, key: &str) -> AppResult<Option<String>> {
        let path = self.key_to_path(key);

        if !path.exists() {
            return Ok(None);
        }

        let mut file = fs::File::open(&path).await?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).await?;
        
        Ok(Some(String::from_utf8_lossy(&contents).to_string()))
    }

    async fn forget(&self, key: &str) -> AppResult<i32> {
        let path = self.key_to_path(key);
        match fs::remove_file(&path).await {
            Ok(_) => Ok(1),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
            Err(e) => Err(crate::Error::from(e)),
        }
    }
}
