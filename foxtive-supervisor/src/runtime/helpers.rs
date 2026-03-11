//! Public helper functions for task supervision

use super::core::TaskRuntime;
use super::types::SupervisionResult;
use crate::contracts::SupervisedTask;
use tokio::task::JoinHandle;

/// Spawn a single supervised task (fire and forget, no dependencies)
pub fn spawn_supervised<T: SupervisedTask + 'static>(task: T) -> JoinHandle<SupervisionResult> {
    TaskRuntime::start_one(task)
}

/// Spawn multiple supervised tasks (no inter-task dependencies)
pub fn spawn_supervised_many<T: SupervisedTask + 'static>(
    tasks: Vec<T>,
) -> Vec<JoinHandle<SupervisionResult>> {
    tasks.into_iter().map(spawn_supervised).collect()
}
