//! Type definitions for the task runtime system

use crate::contracts::SupervisedTask;
use crate::enums::SupervisionStatus;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::broadcast;

/// A boxed future that resolves to `anyhow::Result<()>`, used as a prerequisite
/// gate before the supervisor starts any tasks.
///
/// # Example
/// ```ignore
/// let ready: PrerequisiteFuture = Box::pin(async {
///     server.wait_until_bound().await?;
///     Ok(())
/// });
/// runtime.add_prerequisite("http-server-ready", ready);
/// ```
pub type PrerequisiteFuture =
    Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'static>>;

/// Result of task supervision containing execution metadata
#[derive(Debug, Clone)]
pub struct SupervisionResult {
    pub task_name: String,
    pub task_id: String,
    pub total_attempts: usize,
    pub final_status: SupervisionStatus,
}

/// Internal handle combining a task with its ready signal sender
pub struct TaskEntry {
    pub task: Arc<dyn SupervisedTask>,
    /// Fired (with Ok) when this task's setup completes successfully,
    /// or with Err when setup fails, so dependents can react immediately.
    pub setup_tx: broadcast::Sender<Result<(), String>>,
}