//! # 🦊 Foxtive Supervisor
//!
//! A Rust supervision library that keeps your async tasks running, even when things go wrong.
//! It provides automatic restarts, dependency management, dynamic task management, and deep observability.
//!
//! ## Core Components
//!
//! - [`Supervisor`]: The primary entry point for configuring and starting supervised tasks.
//! - [`TaskRuntime`]: The underlying runtime handle for managing tasks after they've started.
//! - [`SupervisedTask`]: The trait your tasks must implement to be supervised.
//! - [`persistence::TaskStateStore`]: Interface for persisting task state across restarts.
//! - [`SupervisorEventListener`]: Interface for listening to supervisor-wide events.

pub mod contracts;
pub mod enums;
pub mod error;
pub mod runtime;
pub mod persistence;
pub mod hierarchy;
pub mod task_pool;

#[cfg(feature = "distributed")]
pub mod distributed;

use std::future::Future;
use std::sync::Arc;

pub use crate::contracts::{SupervisedTask, SupervisorEventListener};
pub use crate::enums::TaskConfig;
pub use crate::error::{SupervisorError, ValidationError};
pub use crate::persistence::TaskStateStore;
pub use crate::runtime::{SupervisionResult, TaskRuntime, spawn_supervised, spawn_supervised_many};

/// Builder for constructing and starting a supervisor.
///
/// `Supervisor` provides a fluent API for registering tasks, setting up prerequisites,
/// and configuring global runtime options like concurrency limits and persistence.
///
/// # Example
///
/// ```rust
/// use foxtive_supervisor::{Supervisor, SupervisedTask};
///
/// struct MyTask;
/// #[async_trait::async_trait]
/// impl SupervisedTask for MyTask {
///     fn id(&self) -> &'static str { "my-task" }
///     async fn run(&self) -> anyhow::Result<()> { Ok(()) }
/// }
///
/// #[tokio::main]
/// async fn main() {
///     Supervisor::new()
///         .add(MyTask)
///         .start_and_wait_any()
///         .await
///         .unwrap();
/// }
/// ```
pub struct Supervisor {
    runtime: crate::runtime::TaskRuntime,
}

impl Supervisor {
    /// Create a new, empty supervisor.
    pub fn new() -> Self {
        Self {
            runtime: crate::runtime::TaskRuntime::new(),
        }
    }

    /// Add a task to be supervised.
    ///
    /// The task must implement the [`SupervisedTask`] trait.
    #[allow(clippy::should_implement_trait)]
    pub fn add<T: SupervisedTask + 'static>(mut self, task: T) -> Self {
        self.runtime.register(task);
        self
    }

    /// Add multiple tasks of the same type.
    pub fn add_many<T: SupervisedTask + 'static>(mut self, tasks: Vec<T>) -> Self {
        self.runtime.register_many(tasks);
        self
    }

    /// Add a task already wrapped in a `Box`.
    ///
    /// Useful when managing a heterogeneous collection of tasks.
    pub fn add_boxed(mut self, task: Box<dyn SupervisedTask>) -> Self {
        self.runtime.register_boxed(task);
        self
    }

    /// Add a task already wrapped in an `Arc`.
    ///
    /// This is the most efficient way to add a task if you already have an `Arc` handle.
    pub fn add_arc(mut self, task: Arc<dyn SupervisedTask>) -> Self {
        self.runtime.register_arc(task);
        self
    }

    /// Set a global concurrency limit for the supervisor.
    ///
    /// This limits how many tasks can be in their `run()` loop simultaneously.
    /// It's useful for preventing resource exhaustion during massive task restarts.
    pub fn with_global_concurrency_limit(mut self, limit: usize) -> Self {
        self.runtime.with_global_concurrency_limit(limit);
        self
    }

    /// Register an event listener to observe lifecycle events.
    ///
    /// Event listeners receive notifications for task starts, failures, restarts, etc.
    pub fn add_listener(mut self, listener: Arc<dyn SupervisorEventListener>) -> Self {
        self.runtime.add_listener(listener);
        self
    }

    /// Set a custom state store for persisting task states across restarts.
    ///
    /// The supervisor will use this store to load previous attempt counts and failure metadata
    /// before starting tasks, and will update it as tasks run.
    pub fn with_state_store(mut self, store: Arc<dyn TaskStateStore>) -> Self {
        self.runtime.with_state_store(store);
        self
    }

    /// Require a named async gate to resolve before any supervised task starts.
    ///
    /// Prerequisites run sequentially in the order they were registered.
    /// If any prerequisite fails, the supervisor aborts startup.
    pub fn require<F>(mut self, name: &'static str, fut: F) -> Self
    where
        F: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        self.runtime.add_prerequisite(name, Box::pin(fut));
        self
    }

    /// Require a named async gate using a closure.
    pub fn require_fn<F, Fut>(mut self, name: &'static str, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    {
        self.runtime.add_prerequisite_fn(name, f);
        self
    }

    /// Consume the builder and return the underlying [`TaskRuntime`].
    ///
    /// Note: This does not start the tasks. You must call `start_all()` on the returned runtime.
    pub fn runtime(self) -> crate::runtime::TaskRuntime {
        self.runtime
    }

    /// Start all registered tasks and return the [`TaskRuntime`] handle.
    ///
    /// This validates the dependency graph and executes prerequisites before spawning tasks.
    ///
    /// # Errors
    /// Returns [`SupervisorError`] if prerequisites fail or if the dependency graph is invalid.
    pub async fn start(
        mut self,
    ) -> Result<crate::runtime::TaskRuntime, crate::error::SupervisorError> {
        self.runtime.start_all().await?;
        Ok(self.runtime)
    }

    /// Start all tasks and block until the first one terminates.
    ///
    /// # Errors
    /// Returns [`SupervisorError`] if startup validation or prerequisites fail.
    pub async fn start_and_wait_any(
        mut self,
    ) -> Result<SupervisionResult, crate::error::SupervisorError> {
        self.runtime.start_all().await?;
        Ok(self.runtime.wait_any().await)
    }

    /// Start all tasks and block until all have terminated.
    ///
    /// # Errors
    /// Returns [`SupervisorError`] if startup validation or prerequisites fail.
    pub async fn start_and_wait_all(
        mut self,
    ) -> Result<Vec<SupervisionResult>, crate::error::SupervisorError> {
        self.runtime.start_all().await?;
        Ok(self.runtime.wait_all().await)
    }
}

impl Default for Supervisor {
    /// Create a new supervisor with default settings.
    fn default() -> Self {
        Self::new()
    }
}
