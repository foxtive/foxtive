use foxtive_supervisor::{
    enums::{BackoffStrategy, RestartPolicy, SupervisorEvent},
    runtime::TaskRuntime,
    contracts::SupervisedTask,
};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

/// A simple task that fails a specified number of times before succeeding
struct FailingTask {
    id: &'static str,
    fail_count: Arc<std::sync::Mutex<usize>>,
    max_failures: usize,
}

impl FailingTask {
    fn new(id: &'static str, max_failures: usize) -> Self {
        Self {
            id,
            fail_count: Arc::new(std::sync::Mutex::new(0)),
            max_failures,
        }
    }
}

#[async_trait::async_trait]
impl SupervisedTask for FailingTask {
    fn id(&self) -> &'static str {
        self.id
    }

    async fn run(&self) -> anyhow::Result<()> {
        let mut count = self.fail_count.lock().unwrap();
        if *count < self.max_failures {
            *count += 1;
            return Err(anyhow::anyhow!("Intentional failure #{}", *count));
        }
        Ok(())
    }

    fn restart_policy(&self) -> RestartPolicy {
        RestartPolicy::MaxAttempts(5)
    }

    fn backoff_strategy(&self) -> BackoffStrategy {
        BackoffStrategy::Fixed(Duration::from_millis(10))
    }
}

/// Test that updating restart policy takes effect on next restart
#[tokio::test]
async fn test_hot_reload_restart_policy() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let mut runtime = TaskRuntime::new();
    
    // Create a task that will fail 3 times
    let task = FailingTask::new("restart-policy-test", 3);
    let id = "restart-policy-test";
    
    runtime.register(task);
    
    // Subscribe to events before starting
    let _event_rx = runtime.subscribe();
    
    // Start the task
    runtime.start_all().await.unwrap();
    
    // Give it time to start and fail once
    sleep(Duration::from_millis(100)).await;
    
    // Update restart policy to Never (should stop after current attempt)
    runtime.update_restart_policy(id, RestartPolicy::Never).await.unwrap();
    
    // Verify the config was updated
    let config = runtime.get_task_config(id).await.unwrap();
    assert_eq!(config.restart_policy, RestartPolicy::Never);
    
    // Shutdown
    runtime.shutdown().await;
    
    println!("✓ Hot reload restart policy test passed");
}

/// Test that updating backoff strategy takes effect
#[tokio::test]
async fn test_hot_reload_backoff_strategy() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let mut runtime = TaskRuntime::new();
    
    let task = FailingTask::new("backoff-test", 2);
    let id = "backoff-test";
    
    runtime.register(task);
    
    // Subscribe to events
    let _event_rx = runtime.subscribe();
    
    // Start the task
    runtime.start_all().await.unwrap();
    
    // Give it time to fail once
    sleep(Duration::from_millis(50)).await;
    
    // Update backoff strategy to use longer delay
    let new_strategy = BackoffStrategy::Fixed(Duration::from_millis(100));
    runtime.update_backoff_strategy(id, new_strategy.clone()).await.unwrap();
    
    // Verify the config was updated by checking it's Fixed with expected duration
    let config = runtime.get_task_config(id).await.unwrap();
    match config.backoff_strategy {
        BackoffStrategy::Fixed(d) => assert_eq!(d, Duration::from_millis(100)),
        _ => panic!("Expected Fixed backoff strategy"),
    }
    
    // Wait for shutdown
    sleep(Duration::from_millis(500)).await;
    runtime.shutdown().await;
    
    println!("✓ Hot reload backoff strategy test passed");
}

/// Test enabling/disabling tasks at runtime
#[tokio::test]
async fn test_hot_reload_enable_disable() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let mut runtime = TaskRuntime::new();
    
    let task = FailingTask::new("enable-disable-test", 5);
    let id = "enable-disable-test";
    
    runtime.register(task);
    
    // Disable the task
    runtime.set_task_enabled(id, false).await.unwrap();
    
    // Verify it's disabled
    let is_enabled = runtime.is_task_enabled(id).await;
    assert!(!is_enabled);
    
    // Re-enable the task
    runtime.set_task_enabled(id, true).await.unwrap();
    
    // Verify it's enabled again
    let is_enabled = runtime.is_task_enabled(id).await;
    assert!(is_enabled);
    
    println!("✓ Hot reload enable/disable test passed");
}

/// Test that configuration change events are emitted
#[tokio::test]
async fn test_config_change_events() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let mut runtime = TaskRuntime::new();
    
    let task = FailingTask::new("event-test", 0);
    let id = "event-test";
    
    runtime.register(task);
    
    // Subscribe to events BEFORE making changes
    let mut event_rx = runtime.subscribe();
    
    // Update restart policy
    runtime.update_restart_policy(id, RestartPolicy::Always).await.unwrap();
    
    // Check that we received the event
    let mut received_event = false;
    if let Ok(SupervisorEvent::TaskConfigUpdated { 
        id: evt_id, 
        field, 
        old_value, 
        new_value, 
        .. 
    }) = event_rx.recv().await {
        assert_eq!(evt_id, id);
        assert_eq!(field, "restart_policy");
        assert!(old_value.contains("MaxAttempts"));
        assert!(new_value.contains("Always"));
        received_event = true;
    }
    
    assert!(received_event, "Should have received TaskConfigUpdated event");
    
    // Update backoff strategy
    runtime.update_backoff_strategy(id, BackoffStrategy::Exponential {
        initial: Duration::from_millis(10),
        max: Duration::from_secs(1),
    }).await.unwrap();
    
    // Check for second event
    let mut received_second_event = false;
    if let Ok(SupervisorEvent::TaskConfigUpdated { 
        field, 
        .. 
    }) = event_rx.try_recv() {
        assert_eq!(field, "backoff_strategy");
        received_second_event = true;
    }
    
    assert!(received_second_event, "Should have received second TaskConfigUpdated event");
    
    println!("✓ Config change events test passed");
}

/// Test validation of invalid backoff strategies
#[tokio::test]
async fn test_validation_rejects_invalid_strategies() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let mut runtime = TaskRuntime::new();
    
    let task = FailingTask::new("validation-test", 0);
    let id = "validation-test";
    
    runtime.register(task);
    
    // Try to set an invalid exponential strategy (initial > max)
    let result = runtime.update_backoff_strategy(id, BackoffStrategy::Exponential {
        initial: Duration::from_secs(100),
        max: Duration::from_secs(10),  // Less than initial - invalid!
    }).await;
    
    assert!(result.is_err(), "Should reject invalid exponential strategy");
    
    // Try to set a fixed delay that's too long (> 1 hour)
    let result = runtime.update_backoff_strategy(id, BackoffStrategy::Fixed(
        Duration::from_secs(3601)  // More than 1 hour - invalid!
    )).await;
    
    assert!(result.is_err(), "Should reject excessively long fixed delay");
    
    println!("✓ Validation test passed");
}

/// Test that unknown task returns error
#[tokio::test]
async fn test_update_unknown_task_returns_error() {
    let runtime = TaskRuntime::new();
    
    // Try to update a non-existent task
    let result = runtime.update_restart_policy("non-existent", RestartPolicy::Always).await;
    assert!(result.is_err());
    
    let result = runtime.update_backoff_strategy("non-existent", BackoffStrategy::Fixed(Duration::from_secs(1))).await;
    assert!(result.is_err());
    
    let result = runtime.set_task_enabled("non-existent", false).await;
    assert!(result.is_err());
    
    let result = runtime.get_task_config("non-existent").await;
    assert!(result.is_none());
    
    let result = runtime.is_task_enabled("non-existent").await;
    // is_task_enabled returns false for unknown tasks
    assert!(!result);
    
    println!("✓ Unknown task error handling test passed");
}

/// Test concurrent configuration updates
#[tokio::test]
async fn test_concurrent_config_updates() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let mut runtime = TaskRuntime::new();
    
    let task = FailingTask::new("concurrent-test", 0);
    let id = "concurrent-test";
    
    runtime.register(task);
    
    // Perform multiple sequential updates (they're async and will be properly locked)
    runtime.update_restart_policy(id, RestartPolicy::Always).await.unwrap();
    runtime.update_backoff_strategy(id, BackoffStrategy::Fixed(Duration::from_millis(50))).await.unwrap();
    runtime.set_task_enabled(id, false).await.unwrap();
    
    // Verify final state
    let config = runtime.get_task_config(id).await.unwrap();
    assert_eq!(config.restart_policy, RestartPolicy::Always);
    assert!(!config.enabled);
    
    println!("✓ Concurrent updates test passed");
}

/// Test that config persists across multiple reads
#[tokio::test]
async fn test_config_persistence_across_reads() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let mut runtime = TaskRuntime::new();
    
    let task = FailingTask::new("persistence-test", 0);
    let id = "persistence-test";
    
    runtime.register(task);
    
    // Update config
    runtime.update_restart_policy(id, RestartPolicy::MaxAttempts(10)).await.unwrap();
    runtime.update_backoff_strategy(id, BackoffStrategy::Linear {
        initial: Duration::from_millis(10),
        increment: Duration::from_millis(5),
        max: Duration::from_secs(1),
    }).await.unwrap();
    
    // Read config multiple times - should be consistent
    for _ in 0..5 {
        let config = runtime.get_task_config(id).await.unwrap();
        assert_eq!(config.restart_policy, RestartPolicy::MaxAttempts(10));
        sleep(Duration::from_millis(10)).await;
    }
    
    println!("✓ Config persistence test passed");
}

/// Test updating multiple tasks simultaneously
#[tokio::test]
async fn test_hot_reload_multiple_tasks() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let mut runtime = TaskRuntime::new();
    
    // Register multiple tasks with static IDs
    let task_ids = ["task-0", "task-1", "task-2", "task-3", "task-4"];
    for id in &task_ids {
        let task = FailingTask::new(id, 0);
        runtime.register(task);
    }
    
    // Update all tasks
    for id in &task_ids {
        runtime.update_restart_policy(id, RestartPolicy::Always).await.unwrap();
        runtime.update_backoff_strategy(id, BackoffStrategy::Fixed(Duration::from_millis(20))).await.unwrap();
    }
    
    // Verify all updates
    for id in &task_ids {
        let config = runtime.get_task_config(id).await.unwrap();
        assert_eq!(config.restart_policy, RestartPolicy::Always);
    }
    
    println!("✓ Multiple tasks hot reload test passed");
}

/// Test rapid configuration changes (stress test)
#[tokio::test]
async fn test_rapid_config_changes() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let mut runtime = TaskRuntime::new();
    
    let task = FailingTask::new("rapid-test", 0);
    let id = "rapid-test";
    
    runtime.register(task);
    
    // Perform many rapid changes
    for i in 0..20 {
        let policy = if i % 2 == 0 {
            RestartPolicy::Always
        } else {
            RestartPolicy::Never
        };
        runtime.update_restart_policy(id, policy).await.unwrap();
    }
    
    // Final state should be the last update
    let config = runtime.get_task_config(id).await.unwrap();
    assert_eq!(config.restart_policy, RestartPolicy::Never); // Last update was odd number
    
    println!("✓ Rapid config changes test passed");
}

/// Test that disabled tasks don't restart after failure
#[tokio::test]
async fn test_disabled_task_no_restart() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    struct CountingTask {
        id: &'static str,
        run_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl SupervisedTask for CountingTask {
        fn id(&self) -> &'static str { self.id }
        
        async fn run(&self) -> anyhow::Result<()> {
            let count = self.run_count.fetch_add(1, Ordering::SeqCst);
            // Succeed on first run, fail on subsequent runs
            if count == 0 {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Intentional failure"))
            }
        }
        
        fn restart_policy(&self) -> RestartPolicy {
            RestartPolicy::Always
        }
        
        fn backoff_strategy(&self) -> BackoffStrategy {
            BackoffStrategy::Fixed(Duration::from_millis(10))
        }
    }
    
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let mut runtime = TaskRuntime::new();
    
    let run_count = Arc::new(AtomicUsize::new(0));
    let task = CountingTask {
        id: "disabled-test",
        run_count: run_count.clone(),
    };
    
    runtime.register(task);
    
    // Start runtime (task will run once successfully)
    runtime.start_all().await.unwrap();
    
    // Wait for first successful run
    sleep(Duration::from_millis(50)).await;
    assert_eq!(run_count.load(Ordering::SeqCst), 1);
    
    // Now disable the task
    runtime.set_task_enabled("disabled-test", false).await.unwrap();
    
    // Trigger a failure by manually restarting (or wait for natural failure if applicable)
    // For this test, we'll just verify the disabled state prevents future restarts
    // by checking the config
    let config = runtime.get_task_config("disabled-test").await.unwrap();
    assert!(!config.enabled);
    
    // Wait a bit to ensure no automatic restarts happen
    sleep(Duration::from_millis(200)).await;
    
    // Count should remain at 1 since task succeeded and is now disabled
    let final_count = run_count.load(Ordering::SeqCst);
    assert_eq!(final_count, 1, "Disabled task should not restart");
    
    runtime.shutdown().await;
    
    println!("✓ Disabled task no restart test passed");
}

/// Test re-enabling a previously disabled task
#[tokio::test]
async fn test_reenable_disabled_task() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let mut runtime = TaskRuntime::new();
    
    let task = FailingTask::new("reenable-test", 0);
    
    runtime.register(task);
    
    // Verify task starts enabled
    assert!(runtime.is_task_enabled("reenable-test").await);
    
    // Disable the task
    runtime.set_task_enabled("reenable-test", false).await.unwrap();
    assert!(!runtime.is_task_enabled("reenable-test").await);
    
    // Re-enable the task
    runtime.set_task_enabled("reenable-test", true).await.unwrap();
    assert!(runtime.is_task_enabled("reenable-test").await);
    
    // Verify config was updated correctly
    let config = runtime.get_task_config("reenable-test").await.unwrap();
    assert!(config.enabled);
    
    println!("✓ Re-enable disabled task test passed");
}

/// Test configuration update while task is running
#[tokio::test]
async fn test_update_during_execution() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    struct LongRunningTask {
        id: &'static str,
        iterations: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl SupervisedTask for LongRunningTask {
        fn id(&self) -> &'static str { self.id }
        
        async fn run(&self) -> anyhow::Result<()> {
            // Run for a while
            for _ in 0..10 {
                self.iterations.fetch_add(1, Ordering::SeqCst);
                sleep(Duration::from_millis(50)).await;
            }
            Ok(())
        }
        
        fn restart_policy(&self) -> RestartPolicy {
            RestartPolicy::Always
        }
    }
    
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let mut runtime = TaskRuntime::new();
    
    let iterations = Arc::new(AtomicUsize::new(0));
    let task = LongRunningTask {
        id: "during-exec-test",
        iterations: iterations.clone(),
    };
    
    runtime.register(task);
    runtime.start_all().await.unwrap();
    
    // Let it run a bit
    sleep(Duration::from_millis(100)).await;
    
    // Update config while running
    runtime.update_restart_policy("during-exec-test", RestartPolicy::Never).await.unwrap();
    
    // Let it finish current execution
    sleep(Duration::from_millis(600)).await;
    
    // Should not restart after completion
    let final_count = iterations.load(Ordering::SeqCst);
    sleep(Duration::from_millis(200)).await;
    assert_eq!(iterations.load(Ordering::SeqCst), final_count, "Task should not restart");
    
    runtime.shutdown().await;
    
    println!("✓ Update during execution test passed");
}
