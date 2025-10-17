use crate::contracts::SupervisedTask;
use crate::enums::{RestartPolicy, SupervisionStatus};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

pub struct TaskRuntime {
    tasks: Vec<Arc<dyn SupervisedTask>>,
    handles: Vec<JoinHandle<SupervisionResult>>,
}

#[derive(Debug, Clone)]
pub struct SupervisionResult {
    pub task_name: String,
    pub total_attempts: usize,
    pub final_status: SupervisionStatus,
}

impl TaskRuntime {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            handles: Vec::new(),
        }
    }

    /// Register a task for supervision
    pub fn register<T: SupervisedTask + 'static>(&mut self, task: T) -> &mut Self {
        self.tasks.push(Arc::new(task));
        self
    }

    /// Register multiple tasks at once
    pub fn register_many<T: SupervisedTask + 'static>(&mut self, tasks: Vec<T>) -> &mut Self {
        for task in tasks {
            self.register(task);
        }
        self
    }

    /// Start all registered tasks
    pub async fn start_all(&mut self) -> anyhow::Result<()> {
        if self.tasks.is_empty() {
            warn!("[Supervisor] No tasks registered");
            return Ok(());
        }

        info!(
            "[Supervisor] Starting {} supervised tasks...",
            self.tasks.len()
        );

        for task in self.tasks.iter() {
            let handle = Self::supervise(task.clone());
            self.handles.push(handle);
        }

        info!("[Supervisor] All tasks started");
        Ok(())
    }

    /// Start a single task with supervision (static method)
    pub fn start_one<T: SupervisedTask + 'static>(task: T) -> JoinHandle<SupervisionResult> {
        Self::supervise(Arc::new(task))
    }

    /// Wait for the first task to complete (blocks until one task terminates)
    pub async fn wait_any(&mut self) -> SupervisionResult {
        if self.handles.is_empty() {
            warn!("[Supervisor] No tasks to wait for");
            return SupervisionResult {
                task_name: "none".to_string(),
                total_attempts: 0,
                final_status: SupervisionStatus::ManuallyStopped,
            };
        }

        let (result, index, remaining) =
            futures_util::future::select_all(std::mem::take(&mut self.handles)).await;

        self.handles = remaining;

        match result {
            Ok(supervision_result) => {
                error!(
                    "[Supervisor] Task '{}' at index {} terminated: {:?}",
                    supervision_result.task_name, index, supervision_result.final_status
                );
                supervision_result
            }
            Err(join_err) => {
                error!(
                    "[Supervisor] Task at index {} panicked: {:?}",
                    index, join_err
                );
                SupervisionResult {
                    task_name: format!("task_{}", index),
                    total_attempts: 0,
                    final_status: SupervisionStatus::ManuallyStopped,
                }
            }
        }
    }

    /// Wait for all tasks to complete (blocks until all tasks terminate)
    pub async fn wait_all(&mut self) -> Vec<SupervisionResult> {
        let mut results = Vec::new();

        while !self.handles.is_empty() {
            let result = self.wait_any().await;
            results.push(result);
        }

        results
    }

    /// Gracefully shutdown all tasks
    ///
    /// This method:
    /// 1. Aborts all running task handles
    /// 2. Calls `on_shutdown()` on each task for cleanup
    ///
    /// Use for graceful application termination (e.g., SIGTERM handler)
    pub async fn shutdown(self) {
        info!("[Supervisor] Shutting down {} tasks...", self.tasks.len());

        // Abort all handles first
        for handle in &self.handles {
            handle.abort();
        }

        // Call on_shutdown for each task
        for task in &self.tasks {
            let name = task.name();
            info!("[Supervisor] Calling on_shutdown for task '{}'", name);
            task.on_shutdown().await;
        }

        info!("[Supervisor] All tasks shut down");
    }

    /// Get the number of running tasks
    pub fn task_count(&self) -> usize {
        self.handles.len()
    }

    /// Core supervision logic - this is where the magic happens
    fn supervise(task: Arc<dyn SupervisedTask>) -> JoinHandle<SupervisionResult> {
        tokio::spawn(async move {
            let name = task.name();
            let mut loop_iteration = 0;
            let mut actual_runs = 0; // Track actual task executions

            // Run setup phase
            info!("[{name}] Running setup phase...");
            if let Err(e) = task.setup().await {
                error!("[{name}] Setup failed: {e:?}");
                task.cleanup().await;
                return SupervisionResult {
                    task_name: name,
                    total_attempts: 0,
                    final_status: SupervisionStatus::SetupFailed,
                };
            }
            info!("[{name}] Setup complete");

            // Main supervision loop
            loop {
                loop_iteration += 1;

                // Check restart policy BEFORE incrementing actual_runs
                match task.restart_policy() {
                    RestartPolicy::Never => {
                        if loop_iteration > 1 {
                            info!("[{name}] Restart policy is Never, stopping");
                            break;
                        }
                    }
                    RestartPolicy::MaxAttempts(max) => {
                        if loop_iteration > max {
                            warn!("[{name}] Max attempts ({max}) reached, giving up");
                            task.cleanup().await;
                            return SupervisionResult {
                                task_name: name,
                                total_attempts: actual_runs,
                                final_status: SupervisionStatus::MaxAttemptsReached,
                            };
                        }
                    }
                    RestartPolicy::Always => {
                        // Continue forever
                    }
                }

                actual_runs += 1; // Increment only when we're about to run
                info!("[{name}] Starting task (attempt #{actual_runs})");

                // Call on_restart hook (skip on first attempt)
                if actual_runs > 1 {
                    task.on_restart(actual_runs).await;
                }

                // Execute the task in a separate tokio task (catches panics)
                let task_clone = task.clone();
                let result = tokio::spawn(async move { task_clone.run().await }).await;

                // Handle execution result
                let error_message = match result {
                    Ok(Ok(())) => {
                        info!("[{name}] Task completed normally");
                        task.cleanup().await;
                        return SupervisionResult {
                            task_name: name,
                            total_attempts: actual_runs,
                            final_status: SupervisionStatus::CompletedNormally,
                        };
                    }
                    Ok(Err(e)) => {
                        let msg = format!("{:?}", e);
                        task.on_error(&msg, actual_runs).await;
                        msg
                    }
                    Err(join_err) => {
                        let msg = if join_err.is_panic() {
                            format!("Task panicked: {:?}", join_err)
                        } else {
                            "Task was cancelled".to_string()
                        };
                        task.on_panic(&msg, actual_runs).await;
                        msg
                    }
                };

                // Check if we should restart
                if !task.should_restart(actual_runs, &error_message).await {
                    warn!("[{name}] Restart prevented by should_restart hook");
                    task.cleanup().await;
                    return SupervisionResult {
                        task_name: name,
                        total_attempts: actual_runs,
                        final_status: SupervisionStatus::RestartPrevented,
                    };
                }

                // Calculate and apply backoff delay
                let delay = task.backoff_strategy().calculate_delay(actual_runs);
                warn!("[{name}] Waiting {delay:?} before restart (attempt #{actual_runs})");
                tokio::time::sleep(delay).await;
            }

            task.cleanup().await;
            SupervisionResult {
                task_name: name,
                total_attempts: actual_runs,
                final_status: SupervisionStatus::ManuallyStopped,
            }
        })
    }
}

impl Default for TaskRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawn a single supervised task (fire and forget)
///
/// # Example
/// ```ignore
/// let handle = spawn_supervised(MyTask::new());
/// // Task runs in background, automatically restarts on failure
/// ```
pub fn spawn_supervised<T: SupervisedTask + 'static>(task: T) -> JoinHandle<SupervisionResult> {
    TaskRuntime::start_one(task)
}

/// Spawn multiple supervised tasks at once
///
/// # Example
/// ```ignore
/// let handles = spawn_supervised_many(vec![
///     TaskA::new(),
///     TaskB::new(),
///     TaskC::new(),
/// ]);
/// ```
pub fn spawn_supervised_many<T: SupervisedTask + 'static>(
    tasks: Vec<T>,
) -> Vec<JoinHandle<SupervisionResult>> {
    tasks
        .into_iter()
        .map(|task| spawn_supervised(task))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockTask {
        name: String,
        fail_count: AtomicUsize,
        max_fails: usize,
    }

    #[async_trait::async_trait]
    impl SupervisedTask for MockTask {
        fn name(&self) -> String {
            self.name.clone()
        }

        async fn run(&self) -> anyhow::Result<()> {
            let count = self.fail_count.fetch_add(1, Ordering::SeqCst);
            if count < self.max_fails {
                anyhow::bail!("Simulated failure {}", count);
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_supervisor_restarts_on_failure() {
        let task = MockTask {
            name: "test_task".to_string(),
            fail_count: AtomicUsize::new(0),
            max_fails: 3,
        };

        let handle = spawn_supervised(task);
        let result = handle.await.unwrap();

        assert_eq!(result.final_status, SupervisionStatus::CompletedNormally);
        assert_eq!(result.total_attempts, 4); // 3 failures + 1 success
    }
}
