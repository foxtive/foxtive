mod common;
use foxtive_supervisor::Supervisor;
use foxtive_supervisor::contracts::SupervisorEventListener;
use foxtive_supervisor::enums::{CircuitBreakerConfig, SupervisorEvent};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;

#[tokio::test]
async fn test_circuit_breaker_trips_and_resets() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let listener = Arc::new(TestEventListener {
        events: events.clone(),
    });

    struct FlakeyTask {
        fail_count: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for FlakeyTask {
        fn id(&self) -> &'static str {
            "flakey"
        }
        fn circuit_breaker(&self) -> Option<CircuitBreakerConfig> {
            Some(CircuitBreakerConfig {
                failure_threshold: 3,
                reset_timeout: Duration::from_millis(200),
            })
        }
        async fn run(&self) -> anyhow::Result<()> {
            let count = self.fail_count.fetch_add(1, Ordering::SeqCst);
            if count < 5 {
                anyhow::bail!("Simulated failure");
            }
            Ok(())
        }
        fn backoff_strategy(&self) -> foxtive_supervisor::enums::BackoffStrategy {
            foxtive_supervisor::enums::BackoffStrategy::Fixed(Duration::from_millis(10))
        }
    }

    let supervisor = Supervisor::new()
        .add(FlakeyTask {
            fail_count: AtomicUsize::new(0),
        })
        .add_listener(listener);

    let runtime = supervisor.start().await.unwrap();

    // Wait for circuit breaker to trip (after 3 failures)
    tokio::time::sleep(Duration::from_millis(300)).await;

    {
        let captured = events.lock().await;
        assert!(captured.iter().any(
            |e| matches!(e, SupervisorEvent::CircuitBreakerTripped { id, .. } if id == "flakey")
        ));
    }

    // Wait for reset timeout (200ms) and next success
    tokio::time::sleep(Duration::from_millis(2000)).await;

    {
        let captured = events.lock().await;
        assert!(captured.iter().any(
            |e| matches!(e, SupervisorEvent::CircuitBreakerHalfOpen { id, .. } if id == "flakey")
        ));
        assert!(captured.iter().any(
            |e| matches!(e, SupervisorEvent::CircuitBreakerReset { id, .. } if id == "flakey")
        ));
    }

    runtime.shutdown().await;
}

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
