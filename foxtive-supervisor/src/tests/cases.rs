use super::mocks::*;
use crate::contracts::SupervisedTask;
use crate::enums::{BackoffStrategy, RestartPolicy, SupervisionStatus};
use crate::{TaskRuntime, spawn_supervised, spawn_supervised_many};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

#[tokio::test]
async fn test_successful_task_completion() {
    let task = MockTask::new("success_task");
    let handle = spawn_supervised(task);
    let result = handle.await.unwrap();

    assert_eq!(result.task_name, "success_task");
    assert_eq!(result.total_attempts, 1);
    assert_eq!(result.final_status, SupervisionStatus::CompletedNormally);
}

#[tokio::test]
async fn test_task_restarts_on_failure() {
    let task = MockTask::new("retry_task").with_failures(3);
    let handle = spawn_supervised(task);
    let result = handle.await.unwrap();

    assert_eq!(result.final_status, SupervisionStatus::CompletedNormally);
    assert_eq!(result.total_attempts, 4); // 3 failures + 1 success
}

#[tokio::test]
async fn test_task_handles_panic() {
    let task = PanickingTask {
        name: "panic_task".to_string(),
        panic_count: AtomicUsize::new(0),
        max_panics: 2,
    };
    let handle = spawn_supervised(task);
    let result = handle.await.unwrap();

    assert_eq!(result.final_status, SupervisionStatus::CompletedNormally);
    assert_eq!(result.total_attempts, 3); // 2 panics + 1 success
}

// ============================================================================
// Restart Policy Tests
// ============================================================================

#[tokio::test]
async fn test_restart_policy_never() {
    let task = MockTask::new("never_restart")
        .with_failures(5)
        .with_policy(RestartPolicy::Never);

    let handle = spawn_supervised(task);
    let result = handle.await.unwrap();

    assert_eq!(result.total_attempts, 1);
    assert_eq!(result.final_status, SupervisionStatus::ManuallyStopped);
}

#[tokio::test]
async fn test_restart_policy_max_attempts() {
    let task = MockTask::new("max_attempts")
        .with_failures(10)
        .with_policy(RestartPolicy::MaxAttempts(3));

    let handle = spawn_supervised(task);
    let result = handle.await.unwrap();

    assert_eq!(result.total_attempts, 3);
    assert_eq!(result.final_status, SupervisionStatus::MaxAttemptsReached);
}

#[tokio::test]
async fn test_restart_policy_always_succeeds_eventually() {
    let task = MockTask::new("always_restart")
        .with_failures(10)
        .with_policy(RestartPolicy::Always);

    let handle = spawn_supervised(task);
    let result = handle.await.unwrap();

    assert_eq!(result.total_attempts, 11);
    assert_eq!(result.final_status, SupervisionStatus::CompletedNormally);
}

// ============================================================================
// Setup/Cleanup Tests
// ============================================================================

#[tokio::test]
async fn test_setup_failure_prevents_execution() {
    let task = SetupFailTask {
        name: "setup_fail".to_string(),
    };
    let handle = spawn_supervised(task);
    let result = handle.await.unwrap();

    assert_eq!(result.total_attempts, 0);
    assert_eq!(result.final_status, SupervisionStatus::SetupFailed);
}

#[tokio::test]
async fn test_all_hooks_are_called() {
    let task = HookTrackingTask::new("hook_test");
    let setup = task.setup_called.clone();
    let cleanup = task.cleanup_called.clone();
    let restart = task.restart_calls.clone();
    let error = task.error_calls.clone();

    let handle = spawn_supervised(task);
    let result = handle.await.unwrap();

    assert!(setup.load(Ordering::SeqCst), "setup should be called");
    assert!(cleanup.load(Ordering::SeqCst), "cleanup should be called");
    assert_eq!(
        restart.load(Ordering::SeqCst),
        1,
        "restart hook should be called once"
    );
    assert_eq!(
        error.load(Ordering::SeqCst),
        1,
        "error hook should be called once"
    );
    assert_eq!(result.total_attempts, 2);
}

// ============================================================================
// Conditional Restart Tests
// ============================================================================

#[tokio::test]
async fn test_should_restart_prevents_restart() {
    let task = ConditionalRestartTask {
        name: "conditional".to_string(),
        fail_count: AtomicUsize::new(0),
        prevent_restart_after: 3,
    };

    let handle = spawn_supervised(task);
    let result = handle.await.unwrap();

    assert_eq!(result.total_attempts, 3);
    assert_eq!(result.final_status, SupervisionStatus::RestartPrevented);
}

// ============================================================================
// TaskRuntime Tests
// ============================================================================

#[tokio::test]
async fn test_runtime_register_and_start() {
    let mut runtime = TaskRuntime::new();
    runtime.register(MockTask::new("task1"));
    runtime.register(MockTask::new("task2"));

    assert_eq!(runtime.task_count(), 0);
    runtime.start_all().await.unwrap();
    assert_eq!(runtime.task_count(), 2);
}

#[tokio::test]
async fn test_runtime_register_many() {
    let mut runtime = TaskRuntime::new();
    let tasks = vec![
        MockTask::new("task1"),
        MockTask::new("task2"),
        MockTask::new("task3"),
    ];
    runtime.register_many(tasks);
    runtime.start_all().await.unwrap();

    assert_eq!(runtime.task_count(), 3);
}

#[tokio::test]
async fn test_runtime_wait_any() {
    let mut runtime = TaskRuntime::new();
    runtime.register(MockTask::new("fast1"));
    runtime.register(MockTask::new("fast2"));
    runtime.start_all().await.unwrap();

    let result = runtime.wait_any().await;
    assert_eq!(result.final_status, SupervisionStatus::CompletedNormally);
    assert_eq!(runtime.task_count(), 1);
}

#[tokio::test]
async fn test_runtime_wait_all() {
    let mut runtime = TaskRuntime::new();
    runtime.register(MockTask::new("task1"));
    runtime.register(MockTask::new("task2").with_failures(1));
    runtime.register(MockTask::new("task3").with_failures(2));
    runtime.start_all().await.unwrap();

    let results = runtime.wait_all().await;
    assert_eq!(results.len(), 3);
    assert!(
        results
            .iter()
            .all(|r| r.final_status == SupervisionStatus::CompletedNormally)
    );
}

#[tokio::test]
async fn test_runtime_empty_start() {
    let mut runtime = TaskRuntime::new();
    let result = runtime.start_all().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_runtime_wait_any_empty() {
    let mut runtime = TaskRuntime::new();
    let result = runtime.wait_any().await;

    assert_eq!(result.task_name, "none");
    assert_eq!(result.total_attempts, 0);
    assert_eq!(result.final_status, SupervisionStatus::ManuallyStopped);
}

#[tokio::test]
async fn test_runtime_shutdown() {
    let task1 = HookTrackingTask::new("shutdown1");
    let task2 = HookTrackingTask::new("shutdown2");
    let shutdown1 = task1.shutdown_called.clone();
    let shutdown2 = task2.shutdown_called.clone();

    let mut runtime = TaskRuntime::new();
    runtime.register(task1);
    runtime.register(task2);
    runtime.start_all().await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;
    runtime.shutdown().await;

    assert!(
        shutdown1.load(Ordering::SeqCst),
        "task1 shutdown should be called"
    );
    assert!(
        shutdown2.load(Ordering::SeqCst),
        "task2 shutdown should be called"
    );
}

// ============================================================================
// Convenience Function Tests
// ============================================================================

#[tokio::test]
async fn test_spawn_supervised_many() {
    let tasks = vec![
        MockTask::new("bulk1"),
        MockTask::new("bulk2"),
        MockTask::new("bulk3"),
    ];

    let handles = spawn_supervised_many(tasks);
    assert_eq!(handles.len(), 3);

    for handle in handles {
        let result = handle.await.unwrap();
        assert_eq!(result.final_status, SupervisionStatus::CompletedNormally);
    }
}

// ============================================================================
// Backoff Strategy Tests
// ============================================================================

#[tokio::test]
async fn test_backoff_constant() {
    let start = std::time::Instant::now();
    let task = MockTask::new("backoff_const")
        .with_failures(2)
        .with_backoff(BackoffStrategy::Fixed(Duration::from_millis(100)));

    let handle = spawn_supervised(task);
    handle.await.unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed >= Duration::from_millis(200)); // 2 failures * 100ms
}

#[tokio::test]
async fn test_backoff_exponential() {
    let start = std::time::Instant::now();
    let task =
        MockTask::new("backoff_exp")
            .with_failures(3)
            .with_backoff(BackoffStrategy::Exponential {
                initial: Duration::from_millis(50),
                max: Duration::from_secs(10),
            });

    let handle = spawn_supervised(task);
    handle.await.unwrap();
    let elapsed = start.elapsed();

    // 50ms + 100ms + 200ms = 350ms minimum
    assert!(elapsed >= Duration::from_millis(350));
}

// ============================================================================
// Edge Cases
// ============================================================================

#[tokio::test]
async fn test_immediate_success_no_restart() {
    let task = MockTask::new("immediate_success");
    let handle = spawn_supervised(task);
    let result = handle.await.unwrap();

    assert_eq!(result.total_attempts, 1);
}

#[tokio::test]
async fn test_task_name_preserved() {
    let task = MockTask::new("custom_name_123");
    let handle = spawn_supervised(task);
    let result = handle.await.unwrap();

    assert_eq!(result.task_name, "custom_name_123");
}

#[tokio::test]
async fn test_multiple_sequential_failures() {
    let task = MockTask::new("many_fails")
        .with_failures(10)
        .with_policy(RestartPolicy::Always)
        .with_backoff(BackoffStrategy::Fixed(Duration::from_millis(1)));

    let handle = spawn_supervised(task);
    let result = handle.await.unwrap();

    assert_eq!(result.total_attempts, 11);
    assert_eq!(result.final_status, SupervisionStatus::CompletedNormally);
}

#[tokio::test]
async fn test_default_runtime() {
    let runtime = TaskRuntime::default();
    assert_eq!(runtime.task_count(), 0);
}

// ============================================================================
// Long Running Task Tests
// ============================================================================

#[tokio::test]
async fn test_long_running_task_can_be_aborted() {
    let started = Arc::new(AtomicBool::new(false));
    let task = LongRunningTask {
        name: "long_runner".to_string(),
        started: started.clone(),
    };

    let handle = spawn_supervised(task);

    // Wait for task to start
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(started.load(Ordering::SeqCst), "Task should have started");

    // Abort the task
    handle.abort();

    // Verify it was aborted (await returns JoinError)
    let result = handle.await;
    assert!(result.is_err());
    assert!(result.unwrap_err().is_cancelled());
}

#[tokio::test]
async fn test_shutdown_aborts_long_running_tasks() {
    let started1 = Arc::new(AtomicBool::new(false));
    let started2 = Arc::new(AtomicBool::new(false));
    let shutdown1 = Arc::new(AtomicBool::new(false));
    let shutdown2 = Arc::new(AtomicBool::new(false));

    struct TrackedLongTask {
        name: String,
        started: Arc<AtomicBool>,
        shutdown_called: Arc<AtomicBool>,
    }

    #[async_trait::async_trait]
    impl SupervisedTask for TrackedLongTask {
        fn id(&self) -> &'static str {
            "long-task"
        }

        fn name(&self) -> String {
            self.name.clone()
        }

        async fn run(&self) -> anyhow::Result<()> {
            self.started.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(3600)).await;
            Ok(())
        }

        async fn on_shutdown(&self) {
            self.shutdown_called.store(true, Ordering::SeqCst);
        }
    }

    let mut runtime = TaskRuntime::new();
    runtime.register(TrackedLongTask {
        name: "long1".to_string(),
        started: started1.clone(),
        shutdown_called: shutdown1.clone(),
    });
    runtime.register(TrackedLongTask {
        name: "long2".to_string(),
        started: started2.clone(),
        shutdown_called: shutdown2.clone(),
    });

    runtime.start_all().await.unwrap();

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(started1.load(Ordering::SeqCst));
    assert!(started2.load(Ordering::SeqCst));

    // Shutdown should abort and call on_shutdown
    runtime.shutdown().await;

    assert!(
        shutdown1.load(Ordering::SeqCst),
        "on_shutdown should be called for task1"
    );
    assert!(
        shutdown2.load(Ordering::SeqCst),
        "on_shutdown should be called for task2"
    );
}

#[tokio::test]
async fn test_wait_any_with_long_and_short_tasks() {
    let started = Arc::new(AtomicBool::new(false));

    let mut runtime = TaskRuntime::new();
    runtime.register(LongRunningTask {
        name: "long_task".to_string(),
        started: started.clone(),
    });
    runtime.register(MockTask::new("quick_task"));

    runtime.start_all().await.unwrap();
    assert_eq!(runtime.task_count(), 2);

    // wait_any should return the quick task that completes
    let result = runtime.wait_any().await;

    assert_eq!(result.task_name, "quick_task");
    assert_eq!(result.final_status, SupervisionStatus::CompletedNormally);
    assert_eq!(runtime.task_count(), 1); // Long task still running

    // Verify long task actually started
    assert!(started.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_multiple_long_running_tasks_with_shutdown() {
    let mut runtime = TaskRuntime::new();

    for i in 0..5 {
        runtime.register(LongRunningTask {
            name: format!("long_{}", i),
            started: Arc::new(AtomicBool::new(false)),
        });
    }

    runtime.start_all().await.unwrap();
    assert_eq!(runtime.task_count(), 5);

    tokio::time::sleep(Duration::from_millis(100)).await;

    // All should still be running
    assert_eq!(runtime.task_count(), 5);

    // Shutdown should handle all gracefully
    runtime.shutdown().await;
}
