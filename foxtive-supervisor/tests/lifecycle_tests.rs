mod common;
use common::*;
use foxtive_supervisor::Supervisor;
use foxtive_supervisor::enums::SupervisionStatus;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[tokio::test]
async fn test_successful_task_lifecycle() {
    let task = HookTrackingTask::new("lifecycle_test");
    let setup = task.setup_called.clone();
    let cleanup = task.cleanup_called.clone();

    let supervisor = Supervisor::new().add(task);
    let runtime = supervisor.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(setup.load(Ordering::SeqCst));
    runtime.shutdown().await;
    assert!(cleanup.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_task_restart_on_failure() {
    let task = MockTask::new("retry_task").with_failures(2);
    let supervisor = Supervisor::new().add(task);

    let result = supervisor.start_and_wait_any().await.unwrap();

    assert_eq!(result.final_status, SupervisionStatus::CompletedNormally);
    assert_eq!(result.total_attempts, 3); // 2 failures + 1 success
}

#[tokio::test]
async fn test_task_panic_recovery() {
    let task = PanickingTask {
        name: "panic_task".to_string(),
        panic_count: std::sync::atomic::AtomicUsize::new(0),
        max_panics: 2,
    };

    let result = Supervisor::new()
        .add(task)
        .start_and_wait_any()
        .await
        .unwrap();

    assert_eq!(result.final_status, SupervisionStatus::CompletedNormally);
    assert_eq!(result.total_attempts, 3);
}
