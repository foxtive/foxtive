use crate::enums::TaskState;
use serde::{Deserialize, Serialize};

mod fs;
mod memory;

pub use fs::FsStateStore;
pub use memory::InMemoryStateStore;

/// State of a task that can be persisted across supervisor restarts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedTaskState {
    pub task_id: String,
    pub last_run_timestamp_secs: Option<u64>,
    pub last_success_timestamp_secs: Option<u64>,
    pub failure_count: usize,
    pub current_attempt: usize,
    pub current_state: TaskState,
}

/// Trait for components that persist task state
#[async_trait::async_trait]
pub trait TaskStateStore: Send + Sync {
    /// Save the state of a task
    async fn save_state(&self, state: PersistedTaskState) -> anyhow::Result<()>;

    /// Load the state of a task by ID
    async fn load_state(&self, task_id: &str) -> anyhow::Result<Option<PersistedTaskState>>;

    /// Load all saved task states
    async fn load_all_states(&self) -> anyhow::Result<Vec<PersistedTaskState>>;

    /// Delete the state of a task
    async fn delete_state(&self, task_id: &str) -> anyhow::Result<()>;
}
