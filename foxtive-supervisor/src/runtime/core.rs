//! Core TaskRuntime implementation
//!
//! This module contains the [`TaskRuntime`] struct, which is the central component
//! for managing and orchestrating supervised tasks. It handles task registration,
//! dependency resolution, prerequisite execution, and the spawning of supervision loops.

use super::supervision::{supervise, SupervisionParams};
use super::types::{DepSetupReceivers, PrerequisiteFuture, SupervisionResult, TaskEntry};
use super::validation::validate_dependencies;
use crate::contracts::{SupervisedTask, SupervisorEventListener};
use crate::enums::{ControlMessage, HealthStatus, SupervisorEvent, TaskConfig};
use crate::error::SupervisorError;
use crate::persistence::TaskStateStore;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, watch, RwLock, Semaphore};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

#[cfg(feature = "cron")]
use foxtive_cron::Cron;

/// The core runtime for managing supervised tasks.
///
/// `TaskRuntime` is responsible for:
/// - Registering tasks and their configurations.
/// - Managing task lifecycle (start, stop, pause, resume, restart).
/// - Handling global and per-task concurrency limits.
/// - Orchestrating task dependencies and prerequisites.
/// - Distributing supervisor events to registered listeners.
/// - Persisting and restoring task states.
///
/// It is typically created by the [`crate::Supervisor`] builder and then
/// used to interact with the running supervised system.
pub struct TaskRuntime {
    pub(super) tasks: HashMap<&'static str, TaskEntry>,
    pub(super) handles: HashMap<&'static str, JoinHandle<SupervisionResult>>,
    /// Named async gates that must resolve before ANY task starts
    pub(super) prerequisites: Vec<(&'static str, PrerequisiteFuture)>,
    /// Build lookup: task_id -> setup watch sender so dependents can subscribe
    pub(super) setup_signals: HashMap<&'static str, watch::Sender<Option<Result<(), String>>>>,
    /// Global event broadcaster
    pub(super) event_tx: broadcast::Sender<SupervisorEvent>,
    /// Registered event listeners
    pub(super) listeners: Vec<Arc<dyn SupervisorEventListener>>,
    /// Optional persistence store for task state
    pub(super) state_store: Option<Arc<dyn TaskStateStore>>,
    /// Global concurrency limit
    pub(super) global_concurrency_limit: Option<Arc<Semaphore>>,
    /// Per-task concurrency limits
    pub(crate) task_concurrency_limits: HashMap<&'static str, Arc<Semaphore>>,
    /// Hot-reloadable task configurations
    pub(super) task_configs: HashMap<&'static str, Arc<RwLock<TaskConfig>>>,
    #[cfg(feature = "cron")]
    #[allow(dead_code)]
    pub(super) cron: Option<Arc<tokio::sync::Mutex<Cron>>>,
}

impl fmt::Debug for TaskRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskRuntime")
            .field("tasks_count", &self.tasks.len())
            .field("handles_count", &self.handles.len())
            .field("prerequisites_count", &self.prerequisites.len())
            .finish()
    }
}

impl TaskRuntime {
    /// Creates a new, empty `TaskRuntime`.
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        #[allow(unused_mut)]
        let mut runtime = Self {
            tasks: HashMap::new(),
            handles: HashMap::new(),
            prerequisites: Vec::new(),
            setup_signals: HashMap::new(),
            event_tx,
            listeners: Vec::new(),
            state_store: None,
            global_concurrency_limit: None,
            task_concurrency_limits: HashMap::new(),
            task_configs: HashMap::new(),
            #[cfg(feature = "cron")]
            cron: None,
        };
        runtime
    }

    // CONCURRENCY CONTROL

    /// Sets a global concurrency limit for the supervisor.
    ///
    /// This limit applies to all tasks and restricts how many can be actively
    /// running their `run()` method at any given time.
    pub fn with_global_concurrency_limit(&mut self, limit: usize) -> &mut Self {
        self.global_concurrency_limit = Some(Arc::new(Semaphore::new(limit)));
        self
    }

    // TASK REGISTRATION

    /// Registers a task for supervision.
    ///
    /// This method adds the task to the runtime's internal registry but does not
    /// start its execution. Tasks are started when `start_all()` is called.
    pub fn register<T: SupervisedTask + 'static>(&mut self, task: T) -> &mut Self {
        let (setup_tx, _) = watch::channel(None);
        let (control_tx, _) = broadcast::channel(10);
        let id = task.id();

        // Setup per-task concurrency limit if specified
        if let Some(limit) = task.concurrency_limit() {
            self.task_concurrency_limits.insert(id, Arc::new(Semaphore::new(limit)));
        }

        // Initialize hot-reloadable config from task's current configuration
        let config = TaskConfig::from_task(&task);
        self.task_configs.insert(id, Arc::new(RwLock::new(config)));

        self.setup_signals.insert(id, setup_tx.clone());
        self.tasks.insert(
            id,
            TaskEntry {
                task: Arc::new(task),
                setup_tx,
                control_tx,
            },
        );
        // Event will be emitted during start_all() to ensure all listeners are registered first
        self
    }

    /// Registers multiple tasks of the same type at once.
    pub fn register_many<T: SupervisedTask + 'static>(&mut self, tasks: Vec<T>) -> &mut Self {
        for task in tasks {
            self.register(task);
        }
        self
    }

    /// Registers a task from a boxed trait object.
    ///
    /// Useful when managing a heterogeneous collection of tasks.
    pub fn register_boxed(&mut self, task: Box<dyn SupervisedTask>) -> &mut Self {
        let (setup_tx, _) = watch::channel(None);
        let (control_tx, _) = broadcast::channel(10);
        let id = task.id();

        if let Some(limit) = task.concurrency_limit() {
            self.task_concurrency_limits.insert(id, Arc::new(Semaphore::new(limit)));
        }

        // Initialize hot-reloadable config from task's current configuration
        let config = TaskConfig::from_task(task.as_ref());
        self.task_configs.insert(id, Arc::new(RwLock::new(config)));

        self.setup_signals.insert(id, setup_tx.clone());
        self.tasks.insert(
            id,
            TaskEntry {
                task: Arc::from(task),
                setup_tx,
                control_tx,
            },
        );
        // Event will be emitted during start_all() for consistency with register()
        self
    }

    /// Registers a task from an `Arc` (zero extra allocation).
    ///
    /// This is the most efficient way to add a task if you already have an `Arc` handle.
    pub fn register_arc(&mut self, task: Arc<dyn SupervisedTask>) -> &mut Self {
        let (setup_tx, _) = watch::channel(None);
        let (control_tx, _) = broadcast::channel(10);
        let id = task.id();

        if let Some(limit) = task.concurrency_limit() {
            self.task_concurrency_limits.insert(id, Arc::new(Semaphore::new(limit)));
        }

        // Initialize hot-reloadable config from task's current configuration
        let config = TaskConfig::from_task(task.as_ref());
        self.task_configs.insert(id, Arc::new(RwLock::new(config)));

        self.setup_signals.insert(id, setup_tx.clone());
        self.tasks.insert(
            id,
            TaskEntry {
                task,
                setup_tx,
                control_tx,
            },
        );
        // Event will be emitted during start_all() for consistency with register()
        self
    }

    // DYNAMIC TASK MANAGEMENT

    /// Registers and starts a new task at runtime.
    ///
    /// This allows adding tasks to an already running supervisor.
    ///
    /// # Errors
    /// Returns `SupervisorError::InternalError` if a task with the same ID already exists.
    /// Returns `SupervisorError::DependencyValidation` if a declared dependency is unknown.
    pub fn add_task<T: SupervisedTask + 'static>(&mut self, task: T) -> Result<(), SupervisorError> {
        let id = task.id();
        if self.tasks.contains_key(id) {
            return Err(SupervisorError::InternalError(format!("Task {} already exists", id)));
        }

        self.register(task);

        // Validate dependency graph including cycle detection for the newly added task
        let tasks_vec: Vec<&TaskEntry> = self.tasks.values().collect();
        validate_dependencies(&tasks_vec)?;

        let entry = self.tasks.get(id).unwrap();

        // Collect receivers for each declared dependency
        let mut dep_receivers = Vec::new();
        let active_deps = entry.task.active_dependencies();
        for dep_id in &active_deps {
            if let Some(sender) = self.setup_signals.get(dep_id) {
                dep_receivers.push((*dep_id, sender.subscribe()));
            } else {
                return Err(SupervisorError::dependency_validation(
                    id,
                    dep_id,
                    crate::error::ValidationError::UnknownTaskId,
                ));
            }
        }

        let task_limit = self.task_concurrency_limits.get(id).cloned();
        let task_config = self.task_configs.get(id).cloned();

        let params = SupervisionParams {
            task: entry.task.clone(),
            setup_tx: entry.setup_tx.clone(),
            control_rx: entry.control_tx.subscribe(),
            event_tx: self.event_tx.clone(),
            dep_receivers,
            state_store: self.state_store.clone(),
            global_semaphore: self.global_concurrency_limit.clone(),
            task_semaphore: task_limit,
            task_config,
        };

        let handle = supervise(params);
        self.handles.insert(id, handle);

        // Send TaskRegistered event for dynamically added task
        let name = entry.task.name();
        let _ = self.event_tx.send(SupervisorEvent::TaskRegistered { id: id.to_string(), name });

        Ok(())
    }

    /// Manually restarts a task by its ID.
    ///
    /// This sends a `Restart` control message to the task's supervision loop.
    ///
    /// # Errors
    /// Returns `SupervisorError::UnknownTask` if no task with the given ID is found.
    pub fn restart_task(&self, id: &str) -> Result<(), SupervisorError> {
        if let Some(entry) = self.tasks.get(id) {
            let _ = entry.control_tx.send(ControlMessage::Restart);
            Ok(())
        } else {
            Err(SupervisorError::UnknownTask(id.to_string()))
        }
    }

    /// Pauses a task by its ID.
    ///
    /// This sends a `Pause` control message to the task's supervision loop.
    /// A paused task will temporarily stop executing its `run()` method.
    ///
    /// # Errors
    /// Returns `SupervisorError::UnknownTask` if no task with the given ID is found.
    pub fn pause_task(&self, id: &str) -> Result<(), SupervisorError> {
        if let Some(entry) = self.tasks.get(id) {
            let _ = entry.control_tx.send(ControlMessage::Pause);
            Ok(())
        } else {
            Err(SupervisorError::UnknownTask(id.to_string()))
        }
    }

    /// Resumes a paused task by its ID.
    ///
    /// This sends a `Resume` control message to the task's supervision loop,
    /// allowing it to continue execution.
    ///
    /// # Errors
    /// Returns `SupervisorError::UnknownTask` if no task with the given ID is found.
    pub fn resume_task(&self, id: &str) -> Result<(), SupervisorError> {
        if let Some(entry) = self.tasks.get(id) {
            let _ = entry.control_tx.send(ControlMessage::Resume);
            Ok(())
        } else {
            Err(SupervisorError::UnknownTask(id.to_string()))
        }
    }

    /// Resets the circuit breaker for a task by its ID.
    ///
    /// # Errors
    /// Returns `SupervisorError::UnknownTask` if no task with the given ID is found.
    pub fn reset_circuit_breaker(&self, id: &str) -> Result<(), SupervisorError> {
        if let Some(entry) = self.tasks.get(id) {
            let _ = entry.control_tx.send(ControlMessage::ResetCircuitBreaker);
            Ok(())
        } else {
            Err(SupervisorError::UnknownTask(id.to_string()))
        }
    }

    // HOT RELOAD CONFIGURATION

    /// Updates the restart policy for a task at runtime.
    ///
    /// The new policy will take effect on the next restart attempt.
    /// This does not trigger an immediate restart.
    ///
    /// # Arguments
    /// * `id` - The task ID to update
    /// * `new_policy` - The new restart policy to apply
    ///
    /// # Errors
    /// Returns `SupervisorError::UnknownTask` if no task with the given ID is found.
    pub async fn update_restart_policy(
        &self,
        id: &str,
        new_policy: crate::enums::RestartPolicy,
    ) -> Result<(), SupervisorError> {
        let config_lock = self.task_configs.get(id)
            .ok_or_else(|| SupervisorError::UnknownTask(id.to_string()))?;
        
        // Validate the new policy
        Self::validate_restart_policy(&new_policy)?;
        
        let mut config = config_lock.write().await;
        let old_policy = config.restart_policy.clone();
        config.restart_policy = new_policy.clone();
        
        // Emit configuration change event
        if let Some(entry) = self.tasks.get(id) {
            let name = entry.task.name();
            let _ = self.event_tx.send(SupervisorEvent::TaskConfigUpdated {
                id: id.to_string(),
                name,
                field: "restart_policy".to_string(),
                old_value: format!("{:?}", old_policy),
                new_value: format!("{:?}", new_policy),
            });
        }
        
        info!(task_id = %id, ?old_policy, ?new_policy, "Updated restart policy");
        Ok(())
    }

    /// Updates the backoff strategy for a task at runtime.
    ///
    /// The new strategy will take effect on the next restart attempt.
    /// This does not trigger an immediate restart.
    ///
    /// # Arguments
    /// * `id` - The task ID to update
    /// * `new_strategy` - The new backoff strategy to apply
    ///
    /// # Errors
    /// Returns `SupervisorError::UnknownTask` if no task with the given ID is found.
    pub async fn update_backoff_strategy(
        &self,
        id: &str,
        new_strategy: crate::enums::BackoffStrategy,
    ) -> Result<(), SupervisorError> {
        let config_lock = self.task_configs.get(id)
            .ok_or_else(|| SupervisorError::UnknownTask(id.to_string()))?;
        
        // Validate the new strategy
        Self::validate_backoff_strategy(&new_strategy)?;
        
        let mut config = config_lock.write().await;
        let old_strategy = config.backoff_strategy.clone();
        config.backoff_strategy = new_strategy.clone();
        
        // Emit configuration change event
        if let Some(entry) = self.tasks.get(id) {
            let name = entry.task.name();
            let _ = self.event_tx.send(SupervisorEvent::TaskConfigUpdated {
                id: id.to_string(),
                name,
                field: "backoff_strategy".to_string(),
                old_value: format!("{:?}", old_strategy),
                new_value: format!("{:?}", new_strategy),
            });
        }
        
        info!(task_id = %id, ?old_strategy, ?new_strategy, "Updated backoff strategy");
        Ok(())
    }

    /// Enables or disables a task at runtime.
    ///
    /// A disabled task will not restart after failure but will continue running
    /// if already executing. To stop a running task, use `stop_task()` first.
    ///
    /// # Arguments
    /// * `id` - The task ID to enable/disable
    /// * `enabled` - Whether the task should be enabled
    ///
    /// # Errors
    /// Returns `SupervisorError::UnknownTask` if no task with the given ID is found.
    pub async fn set_task_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<(), SupervisorError> {
        let config_lock = self.task_configs.get(id)
            .ok_or_else(|| SupervisorError::UnknownTask(id.to_string()))?;
        
        let mut config = config_lock.write().await;
        let old_enabled = config.enabled;
        config.enabled = enabled;
        
        // Emit configuration change event
        if let Some(entry) = self.tasks.get(id) {
            let name = entry.task.name();
            let _ = self.event_tx.send(SupervisorEvent::TaskConfigUpdated {
                id: id.to_string(),
                name,
                field: "enabled".to_string(),
                old_value: old_enabled.to_string(),
                new_value: enabled.to_string(),
            });
        }
        
        info!(task_id = %id, old_enabled = old_enabled, new_enabled = enabled, "Updated task enabled status");
        Ok(())
    }

    /// Gets the current configuration for a task.
    ///
    /// # Arguments
    /// * `id` - The task ID to query
    ///
    /// # Returns
    /// A clone of the current task configuration, or None if task not found.
    pub async fn get_task_config(&self, id: &str) -> Option<TaskConfig> {
        let config_lock = self.task_configs.get(id)?;
        let config = config_lock.read().await;
        Some(config.clone())
    }

    /// Checks if a task is currently enabled.
    ///
    /// # Arguments
    /// * `id` - The task ID to check
    ///
    /// # Returns
    /// `true` if the task is enabled, `false` otherwise.
    pub async fn is_task_enabled(&self, id: &str) -> bool {
        if let Some(config_lock) = self.task_configs.get(id) {
            let config = config_lock.read().await;
            config.enabled
        } else {
            false
        }
    }

    // TASK GROUP MANAGEMENT

    /// Starts all tasks in a specific group.
    ///
    /// This is useful for atomic operations where related tasks should be started together.
    /// If any task in the group fails to start, the operation continues with other tasks.
    ///
    /// # Arguments
    /// * `group_id` - The group identifier to start
    ///
    /// # Returns
    /// Number of tasks started
    pub fn start_group(&mut self, group_id: &str) -> usize {
        let mut started_count = 0;
        
        // Collect task IDs that belong to this group
        let group_task_ids: Vec<&'static str> = self.tasks.iter()
            .filter(|(_, entry)| entry.task.group_id() == Some(group_id))
            .map(|(id, _)| *id)
            .collect();
        
        for task_id in group_task_ids {
            if !self.handles.contains_key(task_id) {
                // Task is registered but not started, start it now
                if let Err(e) = self.add_task_by_id(task_id) {
                    error!(task_id = %task_id, error = ?e, "Failed to start task in group");
                } else {
                    started_count += 1;
                }
            }
        }
        
        info!(group_id = %group_id, started = started_count, "Started task group");
        started_count
    }

    /// Stops all tasks in a specific group.
    ///
    /// Sends Stop control messages to all tasks in the group and waits for them to terminate.
    ///
    /// # Arguments
    /// * `group_id` - The group identifier to stop
    ///
    /// # Returns
    /// Number of tasks stopped
    pub async fn stop_group(&mut self, group_id: &str) -> usize {
        let mut stopped_count = 0;
        
        // Collect task IDs that belong to this group
        let group_task_ids: Vec<&'static str> = self.tasks.iter()
            .filter(|(_, entry)| entry.task.group_id() == Some(group_id))
            .map(|(id, _)| *id)
            .collect();
        
        for task_id in group_task_ids {
            if let Some(handle) = self.handles.remove(task_id) {
                if let Some(entry) = self.tasks.get(task_id) {
                    let _ = entry.control_tx.send(ControlMessage::Stop);
                }
                
                // Wait briefly for graceful shutdown
                match tokio::time::timeout(Duration::from_secs(5), handle).await {
                    Ok(_) => stopped_count += 1,
                    Err(_) => {
                        warn!(task_id = %task_id, "Task in group did not stop gracefully, aborting");
                        stopped_count += 1; // Count it anyway
                    }
                }
            }
        }
        
        info!(group_id = %group_id, stopped = stopped_count, "Stopped task group");
        stopped_count
    }

    /// Restarts all tasks in a specific group.
    ///
    /// Sends Restart control messages to all running tasks in the group.
    ///
    /// # Arguments
    /// * `group_id` - The group identifier to restart
    ///
    /// # Returns
    /// Number of tasks restarted
    pub fn restart_group(&self, group_id: &str) -> usize {
        let mut restarted_count = 0;
        
        for (task_id, entry) in &self.tasks {
            if entry.task.group_id() == Some(group_id) && self.handles.contains_key(task_id) {
                let _ = entry.control_tx.send(ControlMessage::Restart);
                restarted_count += 1;
            }
        }
        
        info!(group_id = %group_id, restarted = restarted_count, "Restarted task group");
        restarted_count
    }

    /// Lists all task IDs in a specific group.
    ///
    /// # Arguments
    /// * `group_id` - The group identifier to query
    ///
    /// # Returns
    /// Vector of task IDs belonging to the group
    pub fn list_group_tasks(&self, group_id: &str) -> Vec<String> {
        self.tasks.iter()
            .filter(|(_, entry)| entry.task.group_id() == Some(group_id))
            .map(|(id, _)| id.to_string())
            .collect()
    }

    /// Gets the aggregated health status for a task group.
    ///
    /// The group health is determined by the worst health status among all tasks in the group:
    /// - If any task is Unhealthy, the group is Unhealthy
    /// - Else if any task is Degraded, the group is Degraded
    /// - Else if all tasks are Healthy, the group is Healthy
    /// - If no tasks are in the group, returns Unknown
    ///
    /// # Arguments
    /// * `group_id` - The group identifier to query
    ///
    /// # Returns
    /// Aggregated HealthStatus for the group
    pub async fn get_group_health(&self, group_id: &str) -> HealthStatus {
        let mut has_healthy = false;
        let mut has_degraded = false;
        let mut has_unhealthy = false;
        let mut task_count = 0;

        for entry in self.tasks.values() {
            if entry.task.group_id() == Some(group_id) {
                task_count += 1;
                let health = entry.task.health_check().await;
                match health {
                    HealthStatus::Healthy => has_healthy = true,
                    HealthStatus::Degraded { .. } => has_degraded = true,
                    HealthStatus::Unhealthy { .. } => has_unhealthy = true,
                    HealthStatus::Unknown => {}, // Don't affect aggregation
                }
            }
        }

        if task_count == 0 {
            return HealthStatus::Unknown;
        }

        // Return worst status
        if has_unhealthy {
            HealthStatus::Unhealthy { reason: "One or more tasks in group are unhealthy".to_string() }
        } else if has_degraded {
            HealthStatus::Degraded { reason: "One or more tasks in group are degraded".to_string() }
        } else if has_healthy {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unknown
        }
    }

    /// Gets detailed health information for all tasks in a group.
    ///
    /// # Arguments
    /// * `group_id` - The group identifier to query
    ///
    /// # Returns
    /// Vector of TaskSummary for each task in the group
    pub async fn get_group_health_details(&self, group_id: &str) -> Vec<TaskSummary> {
        let mut summaries = Vec::new();
        
        for (task_id, entry) in &self.tasks {
            if entry.task.group_id() == Some(group_id) {
                summaries.push(TaskSummary {
                    id: task_id.to_string(),
                    name: entry.task.name(),
                    health: entry.task.health_check().await,
                });
            }
        }
        
        summaries
    }

    /// Helper method to start a task by ID (used internally for group operations)
    fn add_task_by_id(&mut self, task_id: &'static str) -> Result<(), SupervisorError> {
        let entry = self.tasks.get(task_id).unwrap();

        // Collect receivers for each declared dependency
        let mut dep_receivers = Vec::new();
        let active_deps = entry.task.active_dependencies();
        for dep_id in &active_deps {
            if let Some(sender) = self.setup_signals.get(dep_id) {
                dep_receivers.push((*dep_id, sender.subscribe()));
            } else {
                return Err(SupervisorError::dependency_validation(
                    task_id,
                    dep_id,
                    crate::error::ValidationError::UnknownTaskId,
                ));
            }
        }

        let task_limit = self.task_concurrency_limits.get(task_id).cloned();
        let task_config = self.task_configs.get(task_id).cloned();

        let params = SupervisionParams {
            task: entry.task.clone(),
            setup_tx: entry.setup_tx.clone(),
            control_rx: entry.control_tx.subscribe(),
            event_tx: self.event_tx.clone(),
            dep_receivers,
            state_store: self.state_store.clone(),
            global_semaphore: self.global_concurrency_limit.clone(),
            task_semaphore: task_limit,
            task_config,
        };

        let handle = supervise(params);
        self.handles.insert(task_id, handle);

        let name = entry.task.name();
        let _ = self.event_tx.send(SupervisorEvent::TaskRegistered { id: task_id.to_string(), name });

        Ok(())
    }

    /// Stops and removes a task by its ID.
    ///
    /// This sends a `Stop` control message to the task, waits for it to terminate,
    /// and then removes it from the runtime.
    ///
    /// # Errors
    /// Returns `SupervisorError::UnknownTask` if no task with the given ID is found.
    /// Returns `SupervisorError::InternalError` if the task panicked during removal.
    pub async fn remove_task(&mut self, id: &str) -> Result<Option<SupervisionResult>, SupervisorError> {
        if let Some(entry) = self.tasks.remove(id) {
            let _ = entry.control_tx.send(ControlMessage::Stop);
            let name = entry.task.name();
            self.setup_signals.remove(id);
            let _ = self.event_tx.send(SupervisorEvent::TaskRemoved { id: id.to_string(), name });
            if let Some(handle) = self.handles.remove(id) {
                match handle.await {
                    Ok(res) => Ok(Some(res)),
                    Err(_) => Err(SupervisorError::InternalError(format!("Task {} panicked during removal", id))),
                }
            } else {
                Ok(None)
            }
        } else {
            Err(SupervisorError::UnknownTask(id.to_string()))
        }
    }

    /// Retrieves detailed information about a specific task.
    ///
    /// This includes its ID, name, and current health status.
    ///
    /// # Errors
    /// Returns `SupervisorError::UnknownTask` if no task with the given ID is found.
    pub async fn get_task_info(&self, id: &str) -> Result<TaskSummary, SupervisorError> {
        if let Some(entry) = self.tasks.get(id) {
            let health = entry.task.health_check().await;
            Ok(TaskSummary {
                id: id.to_string(),
                name: entry.task.name(),
                health,
            })
        } else {
            Err(SupervisorError::UnknownTask(id.to_string()))
        }
    }

    /// Lists summaries of all currently registered tasks.
    pub async fn list_tasks(&self) -> Vec<TaskSummary> {
        let mut summaries = Vec::new();
        for (id, entry) in &self.tasks {
            summaries.push(TaskSummary {
                id: id.to_string(),
                name: entry.task.name(),
                health: entry.task.health_check().await,
            });
        }
        summaries
    }

    // PERSISTENCE

    /// Sets a custom state store for persisting task states.
    ///
    /// This method is typically used by the [`crate::Supervisor`] builder.
    pub fn with_state_store(&mut self, store: Arc<dyn TaskStateStore>) -> &mut Self {
        self.state_store = Some(store);
        self
    }

    // EVENT SYSTEM

    /// Subscribes to the supervisor's event stream.
    ///
    /// Returns a `tokio::sync::broadcast::Receiver` that will receive all
    /// [`SupervisorEvent`]s emitted by the runtime.
    pub fn subscribe(&self) -> broadcast::Receiver<SupervisorEvent> {
        self.event_tx.subscribe()
    }

    /// Registers an event listener.
    ///
    /// This method is typically used by the [`crate::Supervisor`] builder.
    pub fn add_listener(&mut self, listener: Arc<dyn SupervisorEventListener>) -> &mut Self {
        self.listeners.push(listener);
        self
    }

    // PREREQUISITE REGISTRATION

    /// Adds an asynchronous prerequisite gate that must resolve before any task starts.
    ///
    /// This method is typically used by the [`crate::Supervisor`] builder.
    pub fn add_prerequisite(&mut self, name: &'static str, fut: PrerequisiteFuture) -> &mut Self {
        self.prerequisites.push((name, fut));
        self
    }

    /// Adds an asynchronous prerequisite gate using a closure.
    ///
    /// This method is typically used by the [`crate::Supervisor`] builder.
    pub fn add_prerequisite_fn<F, Fut>(&mut self, name: &'static str, f: F) -> &mut Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    {
        self.add_prerequisite(name, Box::pin(async move { f().await }))
    }

    // STARTUP

    /// Starts all registered tasks.
    ///
    /// This method first runs all prerequisites, then validates dependencies,
    /// and finally spawns the supervision loop for each task.
    ///
    /// # Errors
    /// Returns [`SupervisorError`] if any prerequisite fails or if the dependency graph is invalid.
    pub async fn start_all(&mut self) -> Result<(), SupervisorError> {
        // Start event listener distribution
        let mut event_rx = self.event_tx.subscribe();
        let listeners = self.listeners.clone();
        tokio::spawn(async move {
            loop {
                match event_rx.recv().await {
                    Ok(event) => {
                        for listener in &listeners {
                            listener.on_event(event.clone()).await;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(missed_events = n, "Event channel lagged, skipping missed events to prevent listener backlog");
                        // Continue listening for new events instead of terminating
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Event channel closed, stopping event distribution");
                        break;
                    }
                }
            }
        });

        // --- Phase 1: prerequisites (run in parallel) ---
        if !self.prerequisites.is_empty() {
            info!("[Supervisor] Running {} prerequisites in parallel...", self.prerequisites.len());
            
            let prereq_futures: Vec<_> = self.prerequisites.drain(..)
                .map(|(name, fut)| async move {
                    info!("[Supervisor] Awaiting prerequisite '{name}'...");
                    fut.await.map_err(|e| (name, e))
                })
                .collect();
            
            let results = futures_util::future::try_join_all(prereq_futures).await
                .map_err(|(name, e)| SupervisorError::prerequisite_failed(name, e))?;
            
            info!("[Supervisor] All {} prerequisites satisfied", results.len());
        }

        if self.tasks.is_empty() {
            warn!("[Supervisor] No tasks registered");
            return Ok(());
        }

        // --- Phase 2: validate dependency graph ---
        let tasks_vec: Vec<&TaskEntry> = self.tasks.values().collect();
        validate_dependencies(&tasks_vec)?;

        info!(
            "[Supervisor] Starting {} supervised tasks...",
            self.tasks.len()
        );

        // Sort tasks by priority (highest first) for startup order
        let mut sorted_ids: Vec<&'static str> = self.tasks.keys().copied().collect();
        sorted_ids.sort_by(|a, b| {
            let task_a = &self.tasks[a].task;
            let task_b = &self.tasks[b].task;
            task_b.priority().cmp(&task_a.priority())
        });

        // --- Phase 3: spawn ---
        for id in sorted_ids {
            let entry = &self.tasks[id];
            // Emit TaskRegistered event during startup (all listeners are registered by now)
            let name = entry.task.name();
            let _ = self.event_tx.send(SupervisorEvent::TaskRegistered { id: id.to_string(), name });

            // Collect receivers for each declared dependency
            let active_deps = entry.task.active_dependencies();
            let dep_receivers: DepSetupReceivers = active_deps
                .iter()
                .map(|dep_id| {
                    let sender = self.setup_signals[dep_id].clone();
                    (*dep_id, sender.subscribe())
                })
                .collect();

            let task_limit = self.task_concurrency_limits.get(id).cloned();
            let task_config = self.task_configs.get(id).cloned();

            let params = SupervisionParams {
                task: entry.task.clone(),
                setup_tx: entry.setup_tx.clone(),
                control_rx: entry.control_tx.subscribe(),
                event_tx: self.event_tx.clone(),
                dep_receivers,
                state_store: self.state_store.clone(),
                global_semaphore: self.global_concurrency_limit.clone(),
                task_semaphore: task_limit,
                task_config,
            };

            let handle = supervise(params);
            self.handles.insert(id, handle);
        }

        info!("[Supervisor] All tasks started");
        Ok(())
    }

    /// Starts a single task with no dependencies (fire and forget).
    ///
    /// This is a convenience function for simple, isolated task supervision
    /// without needing a full `TaskRuntime` setup.
    pub fn start_one<T: SupervisedTask + 'static>(task: T) -> JoinHandle<SupervisionResult> {
        let (setup_tx, _) = watch::channel(None);
        let (control_tx, _) = broadcast::channel(10);
        let (event_tx, _) = broadcast::channel(1);
        let params = SupervisionParams {
            task: Arc::new(task),
            setup_tx,
            control_rx: control_tx.subscribe(),
            event_tx,
            dep_receivers: vec![],
            state_store: None,
            global_semaphore: None,
            task_semaphore: None,
            task_config: None,
        };
        supervise(params)
    }

    // WAITING

    /// Waits for any one supervised task to terminate.
    ///
    /// Returns the `SupervisionResult` of the first task that finishes.
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

        // Extract all handles from the HashMap to avoid borrow issues
        let mut handle_vec: Vec<(&'static str, JoinHandle<SupervisionResult>)> = self.handles.drain().collect();
        
        // Use select_all to wait for the first one to complete
        use futures_util::future::select_all;
        
        let mut futures = Vec::new();
        for (id, handle) in handle_vec.iter_mut() {
            let id_clone = *id;
            futures.push(Box::pin(async move {
                let result = handle.await;
                (id_clone, result)
            }));
        }
        
        let ((finished_id, result), _idx, _remaining) = select_all(futures).await;
        
        // Put remaining handles back into the HashMap
        for (i, (id, handle)) in handle_vec.into_iter().enumerate() {
            if i != _idx {
                self.handles.insert(id, handle);
            }
        }
        
        match result {
            Ok(supervision_result) => {
                error!(
                    "[Supervisor] Task '{}' (id: {}) terminated: {:?}",
                    supervision_result.task_name,
                    supervision_result.task_id,
                    supervision_result.final_status
                );
                supervision_result
            }
            Err(join_err) => {
                error!(
                    "[Supervisor] Task {} panicked: {:?}",
                    finished_id, join_err
                );
                SupervisionResult {
                    task_name: "unknown".to_string(),
                    task_id: finished_id.to_string(),
                    total_attempts: 0,
                    final_status: crate::enums::SupervisionStatus::ManuallyStopped,
                }
            }
        }
    }

    /// Waits for all supervised tasks to terminate.
    ///
    /// Returns a vector of `SupervisionResult` for all tasks.
    pub async fn wait_all(&mut self) -> Vec<SupervisionResult> {
        let mut results = Vec::new();
        while !self.handles.is_empty() {
            results.push(self.wait_any().await);
        }
        results
    }

    /// Initiates a graceful shutdown of all supervised tasks.
    ///
    /// This sends a `Stop` control message to each task and waits for them
    /// to complete their `on_shutdown()` hooks and terminate.
    ///
    /// Shutdown is performed in reverse dependency order (leaves first, then roots).
    pub async fn shutdown(mut self) {
        info!("[Supervisor] Shutting down {} tasks...", self.tasks.len());
        let _ = self.event_tx.send(SupervisorEvent::SupervisorShutdownStarted);

        // Calculate shutdown order based on dependencies.
        // We want to shut down tasks that NO OTHER task depends on first.
        let shutdown_order = self.calculate_shutdown_order();

        for id in shutdown_order {
            if let Some(entry) = self.tasks.get(id) {
                let name = entry.task.name();
                let timeout = entry.task.shutdown_timeout();
                info!(task_id = %id, "Signalling task '{}' to stop...", name);

                let _ = entry.control_tx.send(ControlMessage::Stop);

                if let Some(handle) = self.handles.remove(id) {
                    // Wait for the supervision loop to finish (it will abort run_handle and call cleanup)
                    match tokio::time::timeout(timeout, handle).await {
                        Ok(res) => {
                            match res {
                                Ok(_supervision_res) => {
                                    info!(task_id = %id, "Task '{}' supervision completed.", name);
                                    // Call on_shutdown hook - separate from cleanup(), this runs once during graceful shutdown
                                    match tokio::time::timeout(timeout, entry.task.on_shutdown()).await {
                                        Ok(_) => info!(task_id = %id, "Task '{}' on_shutdown completed.", name),
                                        Err(_) => warn!(task_id = %id, "Task '{}' on_shutdown timed out after {:?}.", name, timeout),
                                    }
                                }
                                Err(e) => error!(task_id = %id, "Task '{}' panicked during shutdown: {:?}", name, e),
                            }
                        }
                        Err(_) => {
                            warn!(task_id = %id, "Task '{}' did not stop within timeout {:?}. Task will continue in background.", name, timeout);
                            // Note: handle was consumed by timeout(), so we can't abort it here.
                            // The task will continue running but shutdown will proceed.
                        }
                    }
                }
            }
        }

        // Clean up any remaining handles (tasks that weren't in the dependency graph somehow)
        for (_id, handle) in self.handles.drain() {
            handle.abort();
        }

        let _ = self.event_tx.send(SupervisorEvent::SupervisorShutdownCompleted);
        info!("[Supervisor] All tasks shut down");
    }

    /// Calculates the order in which tasks should be shut down.
    /// Tasks with no dependents are shut down first.
    fn calculate_shutdown_order(&self) -> Vec<&'static str> {
        let mut order = Vec::new();
        let mut visited = HashSet::new();

        // Build adjacency list for "is depended on by"
        let mut dependents: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
        for (id, entry) in &self.tasks {
            let active_deps = entry.task.active_dependencies();
            for dep_id in &active_deps {
                dependents.entry(dep_id).or_default().push(*id);
            }
        }

        // Helper for DFS to find shutdown order
        fn visit(
            id: &'static str,
            dependents: &HashMap<&'static str, Vec<&'static str>>,
            visited: &mut HashSet<&'static str>,
            order: &mut Vec<&'static str>,
        ) {
            if visited.contains(id) {
                return;
            }

            // Visit all tasks that depend on this task first
            if let Some(deps) = dependents.get(id) {
                for dep in deps {
                    visit(dep, dependents, visited, order);
                }
            }

            visited.insert(id);
            order.push(id);
        }

        for id in self.tasks.keys() {
            visit(id, &dependents, &mut visited, &mut order);
        }

        order
    }

    /// Returns the number of tasks currently being supervised.
    pub fn task_count(&self) -> usize {
        self.handles.len()
    }

    // CONFIGURATION VALIDATION

    /// Validates a restart policy before applying it
    fn validate_restart_policy(_policy: &crate::enums::RestartPolicy) -> Result<(), SupervisorError> {
        // Currently all restart policies are valid
        // Future enhancements could add constraints like:
        // - MaxAttempts must be > 0
        // - Warn if changing from Always to Never on critical tasks
        Ok(())
    }

    /// Validates a backoff strategy before applying it
    fn validate_backoff_strategy(strategy: &crate::enums::BackoffStrategy) -> Result<(), SupervisorError> {
        // Validate that delays are reasonable (not too short, not too long)
        match strategy {
            crate::enums::BackoffStrategy::Fixed(duration) => {
                if duration.as_secs() > 3600 {
                    return Err(SupervisorError::InternalError(
                        "Fixed backoff delay cannot exceed 1 hour".to_string()
                    ));
                }
            }
            crate::enums::BackoffStrategy::Exponential { initial, max } => {
                if initial.as_secs() > 3600 || max.as_secs() > 3600 {
                    return Err(SupervisorError::InternalError(
                        "Exponential backoff delays cannot exceed 1 hour".to_string()
                    ));
                }
                if initial > max {
                    return Err(SupervisorError::InternalError(
                        "Initial backoff cannot be greater than max backoff".to_string()
                    ));
                }
            }
            crate::enums::BackoffStrategy::Linear { initial, increment: _, max } => {
                if initial.as_secs() > 3600 || max.as_secs() > 3600 {
                    return Err(SupervisorError::InternalError(
                        "Linear backoff delays cannot exceed 1 hour".to_string()
                    ));
                }
                if initial > max {
                    return Err(SupervisorError::InternalError(
                        "Initial backoff cannot be greater than max backoff".to_string()
                    ));
                }
            }
            crate::enums::BackoffStrategy::Fibonacci { initial, max } => {
                if initial.as_secs() > 3600 || max.as_secs() > 3600 {
                    return Err(SupervisorError::InternalError(
                        "Fibonacci backoff delays cannot exceed 1 hour".to_string()
                    ));
                }
                if initial > max {
                    return Err(SupervisorError::InternalError(
                        "Initial backoff cannot be greater than max backoff".to_string()
                    ));
                }
            }
            crate::enums::BackoffStrategy::Custom(_) => {
                // Custom strategies can't be validated without executing them
                // Trust the user's implementation
            }
        }
        Ok(())
    }
}

/// A summary of a task's current status.
#[derive(Debug, Clone)]
pub struct TaskSummary {
    pub id: String,
    pub name: String,
    pub health: HealthStatus,
}

impl Default for TaskRuntime {
    fn default() -> Self {
        Self::new()
    }
}
