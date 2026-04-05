use super::{PersistedTaskState, TaskStateStore};
use std::collections::HashMap;
use tokio::sync::Mutex;

/// A default in-memory implementation of TaskStateStore (for reference/testing)
pub struct InMemoryStateStore {
    states: Mutex<HashMap<String, PersistedTaskState>>,
}

impl InMemoryStateStore {
    pub fn new() -> Self {
        Self {
            states: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryStateStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl TaskStateStore for InMemoryStateStore {
    async fn save_state(&self, state: PersistedTaskState) -> anyhow::Result<()> {
        let mut states = self.states.lock().await;
        states.insert(state.task_id.clone(), state);
        Ok(())
    }

    async fn load_state(&self, task_id: &str) -> anyhow::Result<Option<PersistedTaskState>> {
        let states = self.states.lock().await;
        Ok(states.get(task_id).cloned())
    }

    async fn load_all_states(&self) -> anyhow::Result<Vec<PersistedTaskState>> {
        let states = self.states.lock().await;
        Ok(states.values().cloned().collect())
    }

    async fn delete_state(&self, task_id: &str) -> anyhow::Result<()> {
        let mut states = self.states.lock().await;
        states.remove(task_id);
        Ok(())
    }
}
