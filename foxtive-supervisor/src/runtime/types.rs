//! Type definitions for the task runtime system

use crate::contracts::SupervisedTask;
use crate::enums::{ControlMessage, SupervisionStatus};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{broadcast, watch};

/// A boxed future that resolves to `anyhow::Result<()>`, used as a prerequisite
/// gate before the supervisor starts any tasks.
pub type PrerequisiteFuture = Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'static>>;

/// Type alias for dependency setup signal receivers.
/// Each tuple contains (dependency_id, receiver) that signals when setup completes.
pub type DepSetupReceivers = Vec<(&'static str, watch::Receiver<Option<Result<(), String>>>)>;

/// Result of task supervision containing execution metadata
#[derive(Debug, Clone)]
pub struct SupervisionResult {
    pub task_name: String,
    pub task_id: String,
    pub total_attempts: usize,
    pub final_status: SupervisionStatus,
}

/// Internal handle combining a task with its communication channels
pub struct TaskEntry {
    pub task: Arc<dyn SupervisedTask>,
    /// Watch channel sender that signals when this task's setup completes.
    /// Uses watch instead of broadcast to ensure late subscribers can see
    /// the final state (setup success/failure) even if it happened before subscription.
    pub setup_tx: watch::Sender<Option<Result<(), String>>>,
    /// Channel for sending control messages to the supervisor loop for this task
    pub control_tx: broadcast::Sender<ControlMessage>,
}
