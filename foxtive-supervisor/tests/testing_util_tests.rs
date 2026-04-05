mod common;
use common::testing::{TestHarness, TaskAssertions};
use foxtive_supervisor::enums::HealthStatus;
use std::time::Duration;

#[tokio::test]
async fn test_harness_and_assertions() {
    let mut harness = TestHarness::new();
    let mock = harness.add_mock("test_task");

    // Explicitly start the supervisor from the harness
    let runtime = harness.supervisor.take().unwrap().start().await.unwrap();

    let assertions = TaskAssertions::new(&runtime);
    assertions.assert_health("test_task", HealthStatus::Healthy).await;

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(mock.run_count() >= 1);
    assert!(mock.setup_called());
}

#[tokio::test]
async fn test_fake_time_backoff() {
    TestHarness::run_with_fake_time(|| async {
        // Advanced time testing logic would go here
    }).await;
}
