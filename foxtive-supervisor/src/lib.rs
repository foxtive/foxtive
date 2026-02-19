pub mod contracts;
pub mod enums;
pub mod error;
pub mod runtime;
#[cfg(test)]
mod tests;

use std::future::Future;
use std::sync::Arc;

use crate::contracts::SupervisedTask;

pub use crate::runtime::{SupervisionResult, TaskRuntime, spawn_supervised, spawn_supervised_many};
pub use crate::error::{SupervisorError, ValidationError};

/// Builder for constructing and starting a supervisor
///
/// # Basic example
/// ```ignore
/// Supervisor::new()
///     .add(DatabasePool::new())
///     .add(RabbitMQConsumer::new())   // depends on "database" via trait
///     .add(HttpServer::new())
///     .start_and_wait_any()
///     .await?;
/// ```
///
/// # With prerequisites (wait for external readiness before any task starts)
/// ```ignore
/// let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
///
/// Supervisor::new()
///     .require("http-server-bound", async move {
///         ready_rx.await.map_err(|_| anyhow::anyhow!("server never ready"))
///     })
///     .add(MyConsumer::new())
///     .start_and_wait_any()
///     .await?;
/// ```
pub struct Supervisor {
    runtime: crate::runtime::TaskRuntime,
}

impl Supervisor {
    pub fn new() -> Self {
        Self {
            runtime: crate::runtime::TaskRuntime::new(),
        }
    }

    // ==============================================================================
    // TASK REGISTRATION  (builder style, no breaking changes to existing API)
    // ==============================================================================

    /// Add a task (owned value)
    #[allow(clippy::should_implement_trait)]
    pub fn add<T: SupervisedTask + 'static>(mut self, task: T) -> Self {
        self.runtime.register(task);
        self
    }

    /// Add multiple tasks of the same type
    pub fn add_many<T: SupervisedTask + 'static>(mut self, tasks: Vec<T>) -> Self {
        self.runtime.register_many(tasks);
        self
    }

    /// Add a task already wrapped in a Box (useful for mixed-type collections)
    pub fn add_boxed(mut self, task: Box<dyn SupervisedTask>) -> Self {
        self.runtime.register_boxed(task);
        self
    }

    /// Add a task already wrapped in an Arc (zero extra allocation)
    pub fn add_arc(mut self, task: Arc<dyn SupervisedTask>) -> Self {
        self.runtime.register_arc(task);
        self
    }

    // ==============================================================================
    // PREREQUISITES
    // ==============================================================================

    /// Require a named async gate to resolve before any task starts.
    ///
    /// If the future returns `Err`, `start*` methods propagate that error
    /// immediately and no tasks are spawned.
    ///
    /// Prerequisites run sequentially in registration order.
    pub fn require<F>(mut self, name: &'static str, fut: F) -> Self
    where
        F: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        self.runtime.add_prerequisite(name, Box::pin(fut));
        self
    }

    /// Require a named async gate using a closure (alternative to `require`)
    pub fn require_fn<F, Fut>(mut self, name: &'static str, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        self.runtime.add_prerequisite_fn(name, f);
        self
    }

    // ==============================================================================
    // RUNTIME ACCESS & STARTUP
    // ==============================================================================

    /// Consume the builder and return the underlying TaskRuntime
    /// for manual control over waiting / shutdown.
    pub fn runtime(self) -> crate::runtime::TaskRuntime {
        self.runtime
    }

    /// Start all tasks; return the runtime for later use
    pub async fn start(mut self) -> Result<crate::runtime::TaskRuntime, crate::error::SupervisorError> {
        self.runtime.start_all().await?;
        Ok(self.runtime)
    }

    /// Start all tasks and block until the first one terminates
    pub async fn start_and_wait_any(mut self) -> Result<SupervisionResult, crate::error::SupervisorError> {
        self.runtime.start_all().await?;
        Ok(self.runtime.wait_any().await)
    }

    /// Start all tasks and block until all have terminated
    pub async fn start_and_wait_all(mut self) -> Result<Vec<SupervisionResult>, crate::error::SupervisorError> {
        self.runtime.start_all().await?;
        Ok(self.runtime.wait_all().await)
    }
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new()
    }
}