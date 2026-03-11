//! Core TaskRuntime implementation

use super::supervision::supervise;
use super::types::{PrerequisiteFuture, SupervisionResult, TaskEntry};
use super::validation::validate_dependencies;
use crate::contracts::SupervisedTask;
use crate::error::SupervisorError;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

pub struct TaskRuntime {
    pub(super) tasks: Vec<TaskEntry>,
    pub(super) handles: Vec<JoinHandle<SupervisionResult>>,
    /// Named async gates that must resolve before ANY task starts
    pub(super) prerequisites: Vec<(&'static str, PrerequisiteFuture)>,
}

impl TaskRuntime {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            handles: Vec::new(),
            prerequisites: Vec::new(),
        }
    }

    // TASK REGISTRATION

    /// Register a task for supervision
    pub fn register<T: SupervisedTask + 'static>(&mut self, task: T) -> &mut Self {
        let (setup_tx, _) = broadcast::channel(1);
        self.tasks.push(TaskEntry {
            task: Arc::new(task),
            setup_tx,
        });
        self
    }

    /// Register multiple tasks of the same type at once
    pub fn register_many<T: SupervisedTask + 'static>(&mut self, tasks: Vec<T>) -> &mut Self {
        for task in tasks {
            self.register(task);
        }
        self
    }

    /// Register a task from a boxed trait object
    ///
    /// Useful when you have mixed task types in a collection.
    ///
    /// # Example
    /// ```ignore
    /// runtime.register_boxed(Box::new(MyConsumer::new()));
    /// runtime.register_boxed(Box::new(MyServer::new()));
    /// ```
    pub fn register_boxed(&mut self, task: Box<dyn SupervisedTask>) -> &mut Self {
        let (setup_tx, _) = broadcast::channel(1);
        self.tasks.push(TaskEntry {
            task: Arc::from(task),
            setup_tx,
        });
        self
    }

    /// Register a task from an Arc (zero-clone if you already hold one)
    pub fn register_arc(&mut self, task: Arc<dyn SupervisedTask>) -> &mut Self {
        let (setup_tx, _) = broadcast::channel(1);
        self.tasks.push(TaskEntry { task, setup_tx });
        self
    }

    // PREREQUISITE REGISTRATION

    /// Add a named async prerequisite that must resolve with `Ok(())` before
    /// any supervised task is allowed to start.
    ///
    /// If the future resolves with `Err`, `start_all` returns that error
    /// immediately and no tasks are started.
    ///
    /// # Example: wait for HTTP server to bind before starting consumers
    /// ```ignore
    /// let (tx, rx) = tokio::sync::oneshot::channel();
    ///
    /// // Somewhere in your server startup:
    /// // tx.send(()).unwrap();
    ///
    /// runtime.add_prerequisite("http-server-bound", Box::pin(async move {
    ///     rx.await.map_err(|_| anyhow::anyhow!("Server never signalled ready"))
    /// }));
    /// ```
    pub fn add_prerequisite(&mut self, name: &'static str, fut: PrerequisiteFuture) -> &mut Self {
        self.prerequisites.push((name, fut));
        self
    }

    /// Convenience: add a prerequisite from any async closure/block
    ///
    /// # Example
    /// ```ignore
    /// runtime.add_prerequisite_fn("db-migrate", || async {
    ///     db.run_migrations().await
    /// });
    /// ```
    pub fn add_prerequisite_fn<F, Fut>(&mut self, name: &'static str, f: F) -> &mut Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    {
        self.add_prerequisite(name, Box::pin(async move { f().await }))
    }

    // STARTUP

    /// Start all registered tasks, respecting prerequisites and dependency order
    ///
    /// Steps:
    /// 1. Run all prerequisites sequentially — abort on first failure
    /// 2. Validate dependency graph (detect cycles, unknown IDs)
    /// 3. Spawn all tasks; each task internally waits for its deps' setup signals
    pub async fn start_all(&mut self) -> Result<(), SupervisorError> {
        // --- Phase 1: prerequisites ---
        for (name, fut) in self.prerequisites.drain(..) {
            info!("[Supervisor] Awaiting prerequisite '{name}'...");
            fut.await
                .map_err(|e| SupervisorError::prerequisite_failed(name, e))?;
            info!("[Supervisor] Prerequisite '{name}' satisfied");
        }

        if self.tasks.is_empty() {
            warn!("[Supervisor] No tasks registered");
            return Ok(());
        }

        // --- Phase 2: validate dependency graph ---
        validate_dependencies(&self.tasks)?;

        info!(
            "[Supervisor] Starting {} supervised tasks...",
            self.tasks.len()
        );

        // Build lookup: task_id -> setup broadcast sender so dependents can subscribe
        let setup_signals: std::collections::HashMap<
            &'static str,
            broadcast::Sender<Result<(), String>>,
        > = self
            .tasks
            .iter()
            .map(|e| (e.task.id(), e.setup_tx.clone()))
            .collect();

        // --- Phase 3: spawn ---
        for entry in self.tasks.iter() {
            // Collect receivers for each declared dependency
            let dep_receivers: Vec<(&'static str, broadcast::Receiver<Result<(), String>>)> = entry
                .task
                .dependencies()
                .iter()
                .map(|dep_id| {
                    // validated above, safe to unwrap
                    let sender = setup_signals[dep_id].clone();
                    (*dep_id, sender.subscribe())
                })
                .collect();

            let handle = supervise(entry.task.clone(), entry.setup_tx.clone(), dep_receivers);
            self.handles.push(handle);
        }

        info!("[Supervisor] All tasks started");
        Ok(())
    }

    /// Start a single task with no dependencies (fire and forget)
    pub fn start_one<T: SupervisedTask + 'static>(task: T) -> JoinHandle<SupervisionResult> {
        let (setup_tx, _) = broadcast::channel(1);
        supervise(Arc::new(task), setup_tx, vec![])
    }

    // WAITING

    /// Wait for the first task to complete
    pub async fn wait_any(&mut self) -> SupervisionResult {
        if self.handles.is_empty() {
            warn!("[Supervisor] No tasks to wait for");
            return SupervisionResult {
                task_name: "none".to_string(),
                task_id: "none".to_string(),
                total_attempts: 0,
                final_status: crate::enums::SupervisionStatus::ManuallyStopped,
            };
        }

        let (result, index, remaining) =
            futures_util::future::select_all(std::mem::take(&mut self.handles)).await;

        self.handles = remaining;

        match result {
            Ok(supervision_result) => {
                error!(
                    "[Supervisor] Task '{}' (id: {}) at index {} terminated: {:?}",
                    supervision_result.task_name,
                    supervision_result.task_id,
                    index,
                    supervision_result.final_status
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
                    task_id: format!("task_{}", index),
                    total_attempts: 0,
                    final_status: crate::enums::SupervisionStatus::ManuallyStopped,
                }
            }
        }
    }

    /// Wait for all tasks to complete
    pub async fn wait_all(&mut self) -> Vec<SupervisionResult> {
        let mut results = Vec::new();
        while !self.handles.is_empty() {
            results.push(self.wait_any().await);
        }
        results
    }

    /// Gracefully shutdown all tasks
    pub async fn shutdown(self) {
        info!("[Supervisor] Shutting down {} tasks...", self.tasks.len());

        for handle in &self.handles {
            handle.abort();
        }

        for entry in &self.tasks {
            let name = entry.task.name();
            info!("[Supervisor] Calling on_shutdown for task '{}'", name);
            entry.task.on_shutdown().await;
        }

        info!("[Supervisor] All tasks shut down");
    }

    /// Number of currently running task handles
    pub fn task_count(&self) -> usize {
        self.handles.len()
    }
}

impl Default for TaskRuntime {
    fn default() -> Self {
        Self::new()
    }
}
