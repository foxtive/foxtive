mod common;
use common::*;
use foxtive_supervisor::Supervisor;
use foxtive_supervisor::contracts::SupervisorEventListener;
use foxtive_supervisor::enums::SupervisorEvent;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

struct TestEventListener {
    events: Arc<Mutex<Vec<SupervisorEvent>>>,
}

#[async_trait::async_trait]
impl SupervisorEventListener for TestEventListener {
    async fn on_event(&self, event: SupervisorEvent) {
        let mut events = self.events.lock().await;
        events.push(event);
    }
}

#[tokio::test]
async fn test_supervisor_events() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let listener = Arc::new(TestEventListener {
        events: events.clone(),
    });

    let supervisor = Supervisor::new()
        .add(MockTask::new("event_task"))
        .add_listener(listener);

    let mut runtime = supervisor.start().await.unwrap();
    runtime.wait_all().await;

    let captured = events.lock().await;

    // Check for some key events
    assert!(
        captured
            .iter()
            .any(|e| matches!(e, SupervisorEvent::TaskRegistered { id, .. } if id == "event_task"))
    );
    assert!(
        captured
            .iter()
            .any(|e| matches!(e, SupervisorEvent::TaskStarted { id, .. } if id == "event_task"))
    );
    assert!(
        captured
            .iter()
            .any(|e| matches!(e, SupervisorEvent::TaskFinished { id, .. } if id == "event_task"))
    );
}

#[tokio::test]
async fn test_shutdown_events() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let listener = Arc::new(TestEventListener {
        events: events.clone(),
    });

    let supervisor = Supervisor::new()
        .add(MockTask::new("shutdown_task"))
        .add_listener(listener);

    let runtime = supervisor.start().await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    runtime.shutdown().await;
    // Add a small delay to allow the event listener task to process the shutdown events
    tokio::time::sleep(Duration::from_millis(10)).await;

    let captured = events.lock().await;
    assert!(
        captured
            .iter()
            .any(|e| matches!(e, SupervisorEvent::SupervisorShutdownStarted))
    );
    assert!(
        captured
            .iter()
            .any(|e| matches!(e, SupervisorEvent::SupervisorShutdownCompleted))
    );
}
