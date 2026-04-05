use foxtive_supervisor::Supervisor;
use foxtive_supervisor::contracts::{SupervisedTask, SupervisorEventListener};
use foxtive_supervisor::enums::SupervisorEvent;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::info;

pub struct BasicTask {
    id: &'static str,
    fail_count: AtomicUsize,
    max_fails: usize,
}

#[async_trait::async_trait]
impl SupervisedTask for BasicTask {
    fn id(&self) -> &'static str {
        self.id
    }

    async fn run(&self) -> anyhow::Result<()> {
        info!("Running task {}", self.id);

        let count = self.fail_count.fetch_add(1, Ordering::SeqCst);
        if count < self.max_fails {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            anyhow::bail!("Simulated failure {}", count);
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        Ok(())
    }
}

pub struct MyEventListener;

#[async_trait::async_trait]
impl SupervisorEventListener for MyEventListener {
    async fn on_event(&self, event: SupervisorEvent) {
        match event {
            SupervisorEvent::TaskStarted { id, attempt, .. } => {
                info!("🔔 Task {} started (attempt {})", id, attempt);
            }
            SupervisorEvent::TaskFailed { id, error, .. } => {
                info!("❌ Task {} failed: {}", id, error);
            }
            SupervisorEvent::TaskFinished { id, .. } => {
                info!("✅ Task {} finished successfully", id);
            }
            SupervisorEvent::TaskBackoff { id, delay, .. } => {
                info!("⏳ Task {} entering backoff for {:?}", id, delay);
            }
            _ => {
                info!("📣 Event: {:?}", event);
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let supervisor = Supervisor::new()
        .add_listener(Arc::new(MyEventListener))
        .add(BasicTask {
            id: "task-1",
            fail_count: AtomicUsize::new(0),
            max_fails: 1,
        })
        .add(BasicTask {
            id: "task-2",
            fail_count: AtomicUsize::new(0),
            max_fails: 0,
        });

    let results = supervisor
        .start_and_wait_all()
        .await?;

    for result in results {
        info!(
            "Task '{}' finished with {:?} after {} attempts",
            result.task_name, result.final_status, result.total_attempts
        );
    }

    Ok(())
}
