use crate::cache::contract::CacheDriverContract;
use crate::results::AppResult;
use async_trait::async_trait;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::PathBuf;
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
    pub fn new<P: Into<PathBuf>>(base_path: P) -> Self {
        Self {
            base_path: Arc::new(base_path.into()),
            path_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn key_to_path(&self, key: &str) -> PathBuf {
        // Check path cache first
        if let Some(path) = self.path_cache.read().await.get(key) {
            return path.clone();
        }

        // Compute and cache new path
        let safe_key = key.replace([':', '/', '\\', '<', '>', '"', '|', '?', '*'], "_");
        let path = self.base_path.join(format!("{}.cache", safe_key));
        self.path_cache
            .write()
            .await
            .insert(key.to_string(), path.clone());
        path
    }
}

#[async_trait]
impl CacheDriverContract for FilesystemCacheDriver {
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
}
