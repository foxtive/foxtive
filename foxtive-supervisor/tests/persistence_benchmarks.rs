use foxtive_supervisor::persistence::{InMemoryStateStore, FsStateStore, PersistedTaskState, TaskStateStore};
use foxtive_supervisor::enums::TaskState;
use tempfile::tempdir;
use std::time::Instant;
use std::sync::Arc;

#[tokio::test]
async fn benchmark_persistence_impact() {
    let dir = tempdir().unwrap();
    let fs_store = Arc::new(FsStateStore::new(dir.path()).await.unwrap());
    let mem_store = Arc::new(InMemoryStateStore::new());

    let iterations = 1000;

    let state = PersistedTaskState {
        task_id: "bench_task".to_string(),
        last_run_timestamp_secs: Some(123456),
        last_success_timestamp_secs: Some(123450),
        failure_count: 1,
        current_attempt: 2,
        current_state: TaskState::Running,
    };

    // Benchmark In-Memory Store
    let start = Instant::now();
    for _ in 0..iterations {
        mem_store.save_state(state.clone()).await.unwrap();
        let _ = mem_store.load_state("bench_task").await.unwrap();
    }
    let mem_duration = start.elapsed();
    println!("In-Memory Store: {} iterations took {:?}", iterations, mem_duration);

    // Benchmark FS Store
    let start = Instant::now();
    for _ in 0..iterations {
        fs_store.save_state(state.clone()).await.unwrap();
        let _ = fs_store.load_state("bench_task").await.unwrap();
    }
    let fs_duration = start.elapsed();
    println!("FS Store: {} iterations took {:?}", iterations, fs_duration);

    assert!(mem_duration < fs_duration);
}
