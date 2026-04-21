use foxtive_supervisor::enums::TaskState;
use foxtive_supervisor::persistence::{
    FsStateStore, InMemoryStateStore, PersistedTaskState, TaskStateStore,
};
use tempfile::tempdir;

#[tokio::test]
async fn test_in_memory_state_store() {
    let store = InMemoryStateStore::new();
    let state = PersistedTaskState {
        task_id: "test_task".to_string(),
        last_run_timestamp_secs: Some(123456),
        last_success_timestamp_secs: Some(123450),
        failure_count: 1,
        current_attempt: 2,
        current_state: TaskState::Running,
    };

    store.save_state(state.clone()).await.unwrap();

    let loaded = store.load_state("test_task").await.unwrap().unwrap();
    assert_eq!(loaded.task_id, "test_task");
    assert_eq!(loaded.failure_count, 1);

    let all = store.load_all_states().await.unwrap();
    assert_eq!(all.len(), 1);

    store.delete_state("test_task").await.unwrap();
    assert!(store.load_state("test_task").await.unwrap().is_none());
}

#[tokio::test]
async fn test_fs_state_store() {
    let dir = tempdir().unwrap();
    let store = FsStateStore::new(dir.path()).await.unwrap();

    let state = PersistedTaskState {
        task_id: "fs_task".to_string(),
        last_run_timestamp_secs: Some(123456),
        last_success_timestamp_secs: Some(123450),
        failure_count: 5,
        current_attempt: 6,
        current_state: TaskState::Retrying,
    };

    store.save_state(state.clone()).await.unwrap();

    let loaded = store.load_state("fs_task").await.unwrap().unwrap();
    assert_eq!(loaded.task_id, "fs_task");
    assert_eq!(loaded.failure_count, 5);

    let all = store.load_all_states().await.unwrap();
    assert_eq!(all.len(), 1);

    store.delete_state("fs_task").await.unwrap();
    assert!(store.load_state("fs_task").await.unwrap().is_none());
}
