//! Generic supervisor for any long-running async task
//! Works with RabbitMQ consumers, HTTP servers, cron jobs, WebSocket handlers, etc.

pub mod contracts;
pub mod enums;
mod runtime;
#[cfg(test)]
mod tests;

use crate::contracts::SupervisedTask;

pub use runtime::{spawn_supervised, spawn_supervised_many, SupervisionResult, TaskRuntime};

/// Builder pattern for creating and starting a supervisor
///
/// # Example
/// ```ignore
/// use foxtive_supervisor::Supervisor;
///
/// Supervisor::new()
///     .add(RabbitMQConsumer::new())
///     .add(HttpServer::new())
///     .add(CronJob::new())
///     .start()
///     .await?;
/// ```
pub struct Supervisor {
    runtime: TaskRuntime,
}

impl Supervisor {
    pub fn new() -> Self {
        Self {
            runtime: TaskRuntime::new(),
        }
    }

    #[allow(clippy::should_implement_trait)]
    /// Add a task to the supervisor
    pub fn add<T: SupervisedTask + 'static>(mut self, task: T) -> Self {
        self.runtime.register(task);
        self
    }

    /// Add multiple tasks at once
    pub fn add_many<T: SupervisedTask + 'static>(mut self, tasks: Vec<T>) -> Self {
        self.runtime.register_many(tasks);
        self
    }

    /// Get the task runtime
    pub fn runtime(self) -> TaskRuntime {
        self.runtime
    }

    /// Start all tasks and return the task runtime
    pub async fn start(mut self) -> anyhow::Result<TaskRuntime> {
        self.runtime.start_all().await?;
        Ok(self.runtime)
    }

    /// Start all tasks and wait for any to complete
    pub async fn start_and_wait_any(mut self) -> anyhow::Result<SupervisionResult> {
        self.runtime.start_all().await?;
        Ok(self.runtime.wait_any().await)
    }

    /// Start all tasks and wait for all to complete
    pub async fn start_and_wait_all(mut self) -> anyhow::Result<Vec<SupervisionResult>> {
        self.runtime.start_all().await?;
        Ok(self.runtime.wait_all().await)
    }
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new()
    }
}
