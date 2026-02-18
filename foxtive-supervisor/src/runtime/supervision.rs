//! Core supervision logic and task lifecycle management

use super::types::SupervisionResult;
use crate::contracts::SupervisedTask;
use crate::enums::{RestartPolicy, SupervisionStatus};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/// Core supervision loop. Waits for dependency setup signals before running.
pub fn supervise(
    task: Arc<dyn SupervisedTask>,
    setup_tx: broadcast::Sender<Result<(), String>>,
    dep_receivers: Vec<(&'static str, broadcast::Receiver<Result<(), String>>)>,
) -> JoinHandle<SupervisionResult> {
    tokio::spawn(async move {
        let name = task.name();
        let id = task.id();

        // --- Wait for all dependencies to signal setup complete ---
        for (dep_id, mut rx) in dep_receivers {
            info!("[{name}] Waiting for dependency '{dep_id}' to complete setup...");
            match rx.recv().await {
                Ok(result) => match result {
                    Ok(()) => {
                        info!("[{name}] Dependency '{dep_id}' is ready");
                    }
                    Err(e) => {
                        error!(
                            "[{name}] Dependency '{dep_id}' setup failed: {e}. \
                             Aborting task."
                        );
                        // Signal our own failure so our dependents don't hang
                        let _ = setup_tx.send(Err(format!(
                            "Dependency '{dep_id}' failed: {e}"
                        )));
                        return SupervisionResult {
                            task_name: name,
                            task_id: id.to_string(),
                            total_attempts: 0,
                            final_status: SupervisionStatus::DependencyFailed,
                        };
                    }
                },
                Err(_) => {
                    // Sender dropped without sending — treat as failure
                    error!(
                        "[{name}] Dependency '{dep_id}' channel closed unexpectedly. \
                         Aborting task."
                    );
                    let _ = setup_tx.send(Err(format!(
                        "Dependency '{dep_id}' channel closed"
                    )));
                    return SupervisionResult {
                        task_name: name,
                        task_id: id.to_string(),
                        total_attempts: 0,
                        final_status: SupervisionStatus::DependencyFailed,
                    };
                }
            }
        }

        // --- Setup phase ---
        info!("[{name}] Running setup phase...");
        if let Err(e) = task.setup().await {
            let msg = format!("{e:?}");
            error!("[{name}] Setup failed: {msg}");
            // Notify dependents of failure
            let _ = setup_tx.send(Err(msg));
            task.cleanup().await;
            return SupervisionResult {
                task_name: name,
                task_id: id.to_string(),
                total_attempts: 0,
                final_status: SupervisionStatus::SetupFailed,
            };
        }

        // Signal dependents: we're ready
        let _ = setup_tx.send(Ok(()));
        info!("[{name}] Setup complete, signalled dependents");

        // --- Main supervision loop ---
        let mut attempt = 0usize;

        loop {
            // Restart policy check
            match task.restart_policy() {
                RestartPolicy::Never if attempt > 0 => {
                    info!("[{name}] Restart policy is Never, stopping");
                    break;
                }
                RestartPolicy::MaxAttempts(max) if attempt >= max => {
                    warn!("[{name}] Max attempts ({max}) reached, giving up");
                    task.cleanup().await;
                    return SupervisionResult {
                        task_name: name,
                        task_id: id.to_string(),
                        total_attempts: attempt,
                        final_status: SupervisionStatus::MaxAttemptsReached,
                    };
                }
                _ => {}
            }

            attempt += 1;
            info!("[{name}] Starting task (attempt #{attempt})");

            if attempt > 1 {
                task.on_restart(attempt).await;
            }

            // Spawn in a child task to catch panics
            let task_clone = task.clone();
            let result = tokio::spawn(async move { task_clone.run().await }).await;

            let error_message = match result {
                Ok(Ok(())) => {
                    info!("[{name}] Task completed normally");
                    task.cleanup().await;
                    return SupervisionResult {
                        task_name: name,
                        task_id: id.to_string(),
                        total_attempts: attempt,
                        final_status: SupervisionStatus::CompletedNormally,
                    };
                }
                Ok(Err(e)) => {
                    let msg = format!("{e:?}");
                    error!("[{name}] Task error (attempt #{attempt}): {msg}");
                    task.on_error(&msg, attempt).await;
                    msg
                }
                Err(join_err) => {
                    let msg = if join_err.is_panic() {
                        format!("Task panicked: {join_err:?}")
                    } else {
                        "Task was cancelled".to_string()
                    };
                    error!("[{name}] {msg} (attempt #{attempt})");
                    task.on_panic(&msg, attempt).await;
                    msg
                }
            };

            if !task.should_restart(attempt, &error_message).await {
                warn!("[{name}] Restart prevented by should_restart hook");
                task.cleanup().await;
                return SupervisionResult {
                    task_name: name,
                    task_id: id.to_string(),
                    total_attempts: attempt,
                    final_status: SupervisionStatus::RestartPrevented,
                };
            }

            let delay = task.backoff_strategy().calculate_delay(attempt);
            warn!("[{name}] Restarting in {delay:?} (attempt #{attempt})");
            tokio::time::sleep(delay).await;
        }

        task.cleanup().await;
        SupervisionResult {
            task_name: name,
            task_id: id.to_string(),
            total_attempts: attempt,
            final_status: SupervisionStatus::ManuallyStopped,
        }
    })
}