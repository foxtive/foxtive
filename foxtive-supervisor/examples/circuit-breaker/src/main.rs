//! # Circuit Breaker Example
//!
//! This example shows how to configure and use the circuit breaker to prevent
//! a failing task from constantly retrying and potentially overwhelming
//! external resources.

use foxtive_supervisor::enums::{BackoffStrategy, CircuitBreakerConfig, HealthStatus};
use foxtive_supervisor::{SupervisedTask, Supervisor};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tracing::{info, warn};

struct ExternalServiceConsumer {
    fail_count: AtomicUsize,
}

#[async_trait::async_trait]
impl SupervisedTask for ExternalServiceConsumer {
    fn id(&self) -> &'static str {
        "external-consumer"
    }

    fn circuit_breaker(&self) -> Option<CircuitBreakerConfig> {
        Some(CircuitBreakerConfig {
            failure_threshold: 3,
            reset_timeout: Duration::from_secs(5),
        })
    }

    fn backoff_strategy(&self) -> BackoffStrategy {
        BackoffStrategy::Fixed(Duration::from_millis(500))
    }

    async fn run(&self) -> anyhow::Result<()> {
        let count = self.fail_count.fetch_add(1, Ordering::SeqCst);

        if count < 10 {
            warn!(
                "[Consumer] Attempt {}: Simulated service failure",
                count + 1
            );
            anyhow::bail!("Service unavailable");
        }

        info!("[Consumer] Attempt {}: Success!", count + 1);
        tokio::time::sleep(Duration::from_secs(3600)).await;
        Ok(())
    }

    async fn health_check(&self) -> HealthStatus {
        HealthStatus::Healthy
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    info!("Starting Circuit Breaker Example...");

    let supervisor = Supervisor::new().add(ExternalServiceConsumer {
        fail_count: AtomicUsize::new(0),
    });

    let runtime = supervisor.start().await?;

    // Observe the circuit breaker in action
    for i in 0..20 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let info = runtime.get_task_info("external-consumer").await?;
        info!("Tick {}: Task ID={}, Health={:?}", i, info.id, info.health);
    }

    runtime.shutdown().await;
    Ok(())
}
