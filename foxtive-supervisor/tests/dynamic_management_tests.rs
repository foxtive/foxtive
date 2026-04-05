mod common;
use common::*;
use foxtive_supervisor::Supervisor;
use std::time::Duration;

#[tokio::test]
async fn test_add_task_at_runtime() {
    let supervisor = Supervisor::new();
    let mut runtime = supervisor.start().await.unwrap();

    assert_eq!(runtime.list_tasks().await.len(), 0);

    runtime.add_task(MockTask::new("dynamic_task")).unwrap();

    assert_eq!(runtime.list_tasks().await.len(), 1);
    let info = runtime.get_task_info("dynamic_task").await.unwrap();
    assert_eq!(info.id, "dynamic_task");
}

#[tokio::test]
async fn test_remove_task_at_runtime() {
    let supervisor = Supervisor::new().add(MockTask::new("removable"));
    let mut runtime = supervisor.start().await.unwrap();

    assert_eq!(runtime.list_tasks().await.len(), 1);

    let result = runtime.remove_task("removable").await.unwrap();
    assert!(result.is_some());
    assert_eq!(runtime.list_tasks().await.len(), 0);
}

#[tokio::test]
async fn test_restart_task_manually() {
    let task = HookTrackingTask::new("restart_test");
    let restart_calls = task.restart_calls.clone();

    let supervisor = Supervisor::new().add(task);
    let runtime = supervisor.start().await.unwrap();

    // Initial start doesn't count as restart in HookTrackingTask's on_restart
    tokio::time::sleep(Duration::from_millis(50)).await;

    runtime.restart_task("restart_test").unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // One from the MockTask's first failure, and one from manual restart
    assert!(restart_calls.load(std::sync::atomic::Ordering::SeqCst) >= 1);
}
