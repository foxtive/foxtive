# Testing with Foxtive Supervisor

Testing background tasks and supervisors can be tricky due to timing, state, and complex failure scenarios. `foxtive-supervisor` provides a set of utilities to make this easier.

## Using the Test Harness

The `TestHarness` provides a fluent API for setting up a supervisor with mock tasks.

```rust
use foxtive_supervisor::testing::{TestHarness, TaskAssertions};
use foxtive_supervisor::enums::HealthStatus;

#[tokio::test]
async fn test_my_system() {
    let mut harness = TestHarness::new();
    
    // Add a mock task that fails twice then succeeds
    let mock = harness.add_mock("my-task").fail_for(2);
    
    let runtime = harness.supervisor.start().await.unwrap();
    let assertions = TaskAssertions::new(&runtime);
    
    // Wait for tasks to process
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    assertions.assert_health("my-task", HealthStatus::Healthy).await;
    assert!(mock.run_count() >= 3);
}
```

## Fake Time for Backoff Testing

To test exponential backoff or long delays without waiting for real time, use `run_with_fake_time`.

```rust
use foxtive_supervisor::testing::TestHarness;
use tokio::time::advance;

#[tokio::test]
async fn test_long_backoff() {
    TestHarness::run_with_fake_time(async {
        let mut harness = TestHarness::new();
        let mock = harness.add_mock("slow-retry")
            .fail_for(1)
            .with_backoff(BackoffStrategy::Fixed(Duration::from_secs(3600)));
            
        let runtime = harness.supervisor.start().await.unwrap();
        
        // Advance time by 1 hour
        advance(Duration::from_secs(3601)).await;
        
        // Check that the task restarted
        assert_eq!(mock.run_count(), 2);
    }).await;
}
```

## Mocking Prerequisites

You can control exactly when a prerequisite resolves.

```rust
use foxtive_supervisor::testing::MockPrerequisite;

let (gate, resolver) = MockPrerequisite::new("api-gate");

let supervisor = Supervisor::new()
    .require("api-gate", async move { gate.wait().await })
    .add(MyTask);

let mut runtime = supervisor.start().await?;

// MyTask won't start until we do this:
resolver.store(true, Ordering::SeqCst);
```

## Best Practices

1.  **Isolated Tests**: Each test should create its own `Supervisor` or `TestHarness` instance.
2.  **Assert on Events**: Use an `EventListener` in your tests to verify that specific events (like `TaskPanicked` or `CircuitBreakerTripped`) occurred.
3.  **Clean up**: Always call `runtime.shutdown().await` if your test doesn't consume the runtime via `wait_all`.
4.  **Use MockTask**: Leverage `MockTask` to track exactly how many times `setup`, `run`, `cleanup`, and `on_shutdown` were called.
