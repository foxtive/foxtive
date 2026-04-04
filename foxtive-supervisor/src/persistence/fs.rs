use super::{PersistedTaskState, TaskStateStore};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A file system-based implementation of TaskStateStore.
/// Each task's state is stored as a JSON file in a specified directory.
pub struct FsStateStore {
    base_path: PathBuf,
}

impl FsStateStore {
    pub async fn new(base_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = base_path.as_ref().to_path_buf();
        fs::create_dir_all(&path).await?;
        Ok(Self { base_path: path })
    }

    fn state_file_path(&self, task_id: &str) -> PathBuf {
        self.base_path.join(format!("{}.json", task_id))
    }
}

#[async_trait::async_trait]
impl TaskStateStore for FsStateStore {
    async fn save_state(&self, state: PersistedTaskState) -> anyhow::Result<()> {
        let file_path = self.state_file_path(&state.task_id);
        let json = serde_json::to_string_pretty(&state)?;
        let mut file = fs::File::create(&file_path).await?;
        file.write_all(json.as_bytes()).await?;
        file.sync_all().await?; // Ensure data is written to disk
        Ok(())
    }

    async fn load_state(&self, task_id: &str) -> anyhow::Result<Option<PersistedTaskState>> {
        let file_path = self.state_file_path(task_id);
        if !file_path.exists() {
            return Ok(None);
        }

        let mut file = fs::File::open(&file_path).await?;
        let mut json = String::new();
        file.read_to_string(&mut json).await?;

        if json.is_empty() {
            return Ok(None);
        }

        let state: PersistedTaskState = serde_json::from_str(&json)?;
        Ok(Some(state))
    }

    async fn load_all_states(&self) -> anyhow::Result<Vec<PersistedTaskState>> {
        let mut states = Vec::new();
        let mut entries = fs::read_dir(&self.base_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                let mut file = fs::File::open(&path).await?;
                let mut json = String::new();
                file.read_to_string(&mut json).await?;

                if json.is_empty() {
                    continue;
                }

                if let Ok(state) = serde_json::from_str::<PersistedTaskState>(&json) {
                    states.push(state);
                } else {
                    // Log error for malformed state files, but don't fail the whole operation
                    eprintln!("Warning: Could not parse state file: {:?}", path);
                }
            }
        }
        Ok(states)
    }

    async fn delete_state(&self, task_id: &str) -> anyhow::Result<()> {
        let file_path = self.state_file_path(task_id);
        if file_path.exists() {
            fs::remove_file(&file_path).await?;
        }
        Ok(())
    }
}
