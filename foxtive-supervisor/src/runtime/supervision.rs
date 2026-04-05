//! Core supervision logic and task lifecycle management

use super::types::{DepSetupReceivers, SupervisionResult};
use crate::contracts::SupervisedTask;
use crate::enums::{ControlMessage, RestartPolicy, SupervisionStatus, SupervisorEvent, TaskConfig, TaskState};
use crate::persistence::{PersistedTaskState, TaskStateStore};
use crate::runtime::circuit_breaker::{CircuitBreaker, CircuitState};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, watch, RwLock, Semaphore};
use tokio::task::JoinHandle;
use tracing::{error, info, info_span, warn, Instrument};

/// Parameters for the supervision loop
pub struct SupervisionParams {
    pub task: Arc<dyn SupervisedTask>,
    pub setup_tx: watch::Sender<Option<Result<(), String>>>,
    pub control_rx: broadcast::Receiver<ControlMessage>,
    pub event_tx: broadcast::Sender<SupervisorEvent>,
    pub dep_receivers: DepSetupReceivers,
    pub state_store: Option<Arc<dyn TaskStateStore>>,
    pub global_semaphore: Option<Arc<Semaphore>>,
    pub task_semaphore: Option<Arc<Semaphore>>,
    /// Hot-reloadable task configuration
    pub task_config: Option<Arc<RwLock<TaskConfig>>>,
}

/// Core supervision loop. Waits for dependency setup signals before running.
pub fn supervise(params: SupervisionParams) -> JoinHandle<SupervisionResult> {
    let SupervisionParams {
        task,
        setup_tx,
        mut control_rx,
        event_tx,
        dep_receivers,
        state_store,
        global_semaphore,
        task_semaphore,
        task_config,
    } = params;

    let name = task.name();
    let id = task.id();

    let supervision_span = info_span!(
        "supervision",
        task_id = id,
        task_name = %name
    );

    tokio::spawn(async move {
        // --- Restore state if store exists ---
        let mut attempt = 0usize;
        let mut failure_count = 0usize;
        let mut last_success_timestamp_secs = None;

        if let Some(store) = &state_store {
            match store.load_state(id).await {
                Ok(Some(persisted)) => {
                    info!(
                        "Restored state for task {}: attempt={}, failures={}",
                        id, persisted.current_attempt, persisted.failure_count
                    );
                    attempt = persisted.current_attempt;
                    failure_count = persisted.failure_count;
                    last_success_timestamp_secs = persisted.last_success_timestamp_secs;
                }
                Ok(None) => info!("No persisted state found for task {}", id),
                Err(e) => error!("Failed to load state for task {}: {:?}", id, e),
            }
        }

        // --- Initialize Circuit Breaker ---
        let mut circuit_breaker = task.circuit_breaker().map(|config| {
            CircuitBreaker::new(config, event_tx.clone(), id.to_string(), name.clone())
        });

        // Wait for all dependencies to complete their setup
        for (dep_id, mut rx) in dep_receivers {
            match wait_for_dependency(dep_id, &mut rx, id).await {
                Ok(()) => info!(dependency = dep_id, "Dependency ready"),
                Err(reason) => {
                    error!(dependency = dep_id, reason, "Dependency failed - aborting task");
                    let _ = setup_tx.send(Some(Err(reason.clone())));
                    return SupervisionResult {
                        task_name: name,
                        task_id: id.to_string(),
                        total_attempts: attempt,
                        final_status: SupervisionStatus::DependencyFailed,
                    };
                }
            }
        }

        // --- Setup phase ---
        let setup_result = {
            let setup_span = info_span!("task_setup");
            let event_tx_clone = event_tx.clone();
            let task_clone = task.clone();
            let name_clone = name.clone();
            async move {
                info!("Running setup phase");
                let _ = event_tx_clone.send(SupervisorEvent::TaskSetupStarted { id: id.to_string(), name: name_clone });
                task_clone.setup().await
            }.instrument(setup_span).await
        };

        if let Err(e) = setup_result {
            let msg = format!("{e:?}");
            error!(error = %msg, "Setup failed");
            let _ = event_tx.send(SupervisorEvent::TaskSetupFailed { id: id.to_string(), name: name.clone(), error: msg.clone() });
            let _ = setup_tx.send(Some(Err(msg)));
            // cleanup() is called after every task termination (success, failure, or panic)
            task.cleanup().await;
            return SupervisionResult {
                task_name: name,
                task_id: id.to_string(),
                total_attempts: attempt,
                final_status: SupervisionStatus::SetupFailed,
            };
        }

        // Signal dependents: we're ready
        let _ = setup_tx.send(Some(Ok(())));
        let _ = event_tx.send(SupervisorEvent::TaskSetupCompleted { id: id.to_string(), name: name.clone() });
        info!("Setup complete, signalled dependents");

        // Apply initial delay if configured
        if let Some(delay) = task.initial_delay()
            && !delay.is_zero() {
            // Calculate actual delay with optional jitter
            #[allow(unused_mut)]
            let mut actual_delay = delay;
            
            #[cfg(feature = "cron")]
            if let Some((min_jitter, max_jitter)) = task.jitter() {
                use std::time::Duration;
                
                let min_ms = min_jitter.as_millis();
                let max_ms = max_jitter.as_millis();
                
                if max_ms > min_ms {
                    let jitter_ms = rand::random_range(min_ms as u64..=max_ms as u64);
                    actual_delay += Duration::from_millis(jitter_ms);
                    info!(base_delay_ms = delay.as_millis(), jitter_ms, total_delay_ms = actual_delay.as_millis(), "Applied jitter to initial delay");
                }
            }
            
            info!(delay_ms = actual_delay.as_millis(), "Applying initial delay before first execution");
            tokio::select! {
                _ = tokio::time::sleep(actual_delay) => {
                    info!("Initial delay completed");
                }
                msg = control_rx.recv() => {
                    if let Ok(ControlMessage::Stop) = msg {
                        info!("Received Stop command during initial delay");
                        let _ = event_tx.send(SupervisorEvent::TaskStopped { id: id.to_string(), name: name.clone() });
                        task.cleanup().await;
                        return SupervisionResult {
                            task_name: name,
                            task_id: id.to_string(),
                            total_attempts: attempt,
                            final_status: SupervisionStatus::ManuallyStopped,
                        };
                    }
                }
            }
        }

        // --- Main supervision loop ---
        let mut is_paused = false;

        loop {
            // Update and persist state
            if let Some(store) = &state_store {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                let state = PersistedTaskState {
                    task_id: id.to_string(),
                    last_run_timestamp_secs: Some(now),
                    last_success_timestamp_secs,
                    failure_count,
                    current_attempt: attempt,
                    current_state: if is_paused {
                        TaskState::Paused
                    } else if circuit_breaker.as_ref().is_some_and(|cb| matches!(cb.state(), CircuitState::Open { .. })) {
                        TaskState::CircuitBreakerOpen
                    } else {
                        TaskState::Running
                    },
                };
                if let Err(e) = store.save_state(state).await {
                    error!("Failed to save state for task {}: {:?}", id, e);
                }
            }

            // Process any pending control messages
            if let Some(control_action) = process_control_messages(
                &mut control_rx,
                &event_tx,
                id,
                &name,
                &task,
                &mut is_paused,
                &mut circuit_breaker,
                attempt,
            ).await {
                return control_action;
            }

            if is_paused {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }

            // Circuit Breaker Check
            if let Some(cb) = &mut circuit_breaker
                && !cb.allow_request() {
                // Use circuit breaker's configured reset_timeout instead of hardcoded delay
                let reset_timeout = cb.reset_timeout();
                info!(timeout_ms = reset_timeout.as_millis(), "Circuit breaker is open, waiting for reset timeout");
                
                // Wait for reset timeout while remaining responsive to control messages
                tokio::select! {
                    _ = tokio::time::sleep(reset_timeout) => {
                        info!("Circuit breaker reset timeout elapsed, will attempt recovery");
                    }
                    msg = control_rx.recv() => {
                        match msg {
                            Ok(ControlMessage::Stop) => {
                                info!("Received Stop command during circuit breaker wait");
                                let _ = event_tx.send(SupervisorEvent::TaskStopped { 
                                    id: id.to_string(), 
                                    name: name.clone() 
                                });
                                task.cleanup().await;
                                return SupervisionResult {
                                    task_name: name,
                                    task_id: id.to_string(),
                                    total_attempts: attempt,
                                    final_status: SupervisionStatus::ManuallyStopped,
                                };
                            }
                            Ok(ControlMessage::ResetCircuitBreaker) => {
                                info!("Received ResetCircuitBreaker command");
                                if let Some(ref mut cb) = circuit_breaker {
                                    cb.reset();
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!(missed_messages = n, "Control channel lagged during circuit breaker wait, messages skipped");
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                warn!("Control channel closed during circuit breaker wait");
                            }
                            _ => {}
                        }
                    }
                }
                continue;
            }

            // Restart policy check
            let restart_policy = if let Some(config_lock) = &task_config {
                let config = config_lock.read().await;
                config.restart_policy.clone()
            } else {
                task.restart_policy()
            };
            
            match restart_policy {
                RestartPolicy::Never if attempt > 0 => {
                    info!("Restart policy is Never, stopping");
                    break;
                }
                RestartPolicy::MaxAttempts(max) if attempt >= max => {
                    warn!(max_attempts = max, "Max attempts reached, giving up");
                    let _ = event_tx.send(SupervisorEvent::TaskMaxAttemptsReached { id: id.to_string(), name: name.clone(), attempts: attempt });
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

            let run_span = info_span!(
                "task_run",
                attempt = attempt,
                correlation_id = %uuid::Uuid::new_v4()
            );

            let _ = event_tx.send(SupervisorEvent::TaskStarted { id: id.to_string(), name: name.clone(), attempt });

            if attempt > 1 {
                let restart_hook_span = info_span!("on_restart_hook");
                task.on_restart(attempt).instrument(restart_hook_span).await;
            }

            // Concurrency Control: Acquire permits
            let _global_permit = if let Some(sem) = &global_semaphore {
                info!("Acquiring global concurrency permit");
                Some(sem.acquire().await.unwrap())
            } else {
                None
            };

            let _task_permit = if let Some(sem) = &task_semaphore {
                info!("Acquiring task-specific concurrency permit");
                Some(sem.acquire().await.unwrap())
            } else {
                None
            };

            // Spawn in a child task to catch panics
            let task_clone = task.clone();
            let mut run_handle = tokio::spawn(
                async move { task_clone.run().await }
                    .instrument(run_span.clone())
            );

            // Wait for task completion or control messages
            let result = loop {
                tokio::select! {
                    res = &mut run_handle => break Some(res),
                    msg = control_rx.recv() => {
                        match handle_control_message_during_execution(
                            msg,
                            &event_tx,
                            id,
                            &name,
                            &task,
                            &mut is_paused,
                            &mut circuit_breaker,
                            &mut run_handle,
                            attempt,
                        ).await {
                            Some(action) => return action,
                            None => {
                                // Check if this was a restart command (run_handle was aborted)
                                if run_handle.is_finished() {
                                    break None; // Trigger restart
                                }
                            }
                        }
                    }
                }
            };

            // Handle task execution result
            if let Some(res) = result {
                match handle_task_result(
                    res,
                    &event_tx,
                    id,
                    &name,
                    &task,
                    attempt,
                    &mut failure_count,
                    &mut last_success_timestamp_secs,
                    &mut circuit_breaker,
                ).await {
                    TaskResultAction::Complete(status) => {
                        // For cron-scheduled tasks, continue looping for next scheduled run
                        #[cfg(feature = "cron")]
                        if task.cron_schedule().is_some() {
                            info!("Cron task completed, waiting for next scheduled execution");
                            continue;
                        }
                        return status;
                    }
                    TaskResultAction::Continue => {},
                    TaskResultAction::RestartPrevented => {
                        task.cleanup().await;
                        return SupervisionResult {
                            task_name: name,
                            task_id: id.to_string(),
                            total_attempts: attempt,
                            final_status: SupervisionStatus::RestartPrevented,
                        };
                    }
                }
            }

            // If circuit is open, we don't proceed to backoff sleep (the loop will continue and be blocked by CB check)
            if circuit_breaker.as_ref().is_some_and(|cb| matches!(cb.state(), CircuitState::Open { .. })) {
                continue;
            }

            // For cron-scheduled tasks, wait until next scheduled execution time
            #[cfg(feature = "cron")]
            if let Some(schedule_str) = task.cron_schedule() {
                use foxtive_cron::contracts::ValidatedSchedule;
                use chrono::Utc;
                
                match ValidatedSchedule::parse(schedule_str) {
                    Ok(schedule) => {
                        let now = Utc::now();
                        if let Some(next_run) = schedule.next_after(&now, chrono_tz::UTC) {
                            let duration = next_run.signed_duration_since(now);
                            if duration.num_milliseconds() > 0 {
                                info!(
                                    next_run = %next_run,
                                    delay_ms = duration.num_milliseconds(),
                                    "Waiting for next cron scheduled execution"
                                );
                                
                                let sleep_duration = std::time::Duration::from_millis(duration.num_milliseconds() as u64);
                                
                                // Sleep but remain responsive to control messages
                                tokio::select! {
                                    _ = tokio::time::sleep(sleep_duration) => {
                                        info!("Cron schedule time reached, will execute task");
                                    }
                                    msg = control_rx.recv() => {
                                        match msg {
                                            Ok(ControlMessage::Stop) => {
                                                info!("Received Stop command while waiting for cron schedule");
                                                let _ = event_tx.send(SupervisorEvent::TaskStopped { 
                                                    id: id.to_string(), 
                                                    name: name.clone() 
                                                });
                                                task.cleanup().await;
                                                return SupervisionResult {
                                                    task_name: name,
                                                    task_id: id.to_string(),
                                                    total_attempts: attempt,
                                                    final_status: SupervisionStatus::ManuallyStopped,
                                                };
                                            }
                                            Ok(ControlMessage::Pause) => {
                                                info!("Received Pause command during cron wait");
                                                let _ = event_tx.send(SupervisorEvent::TaskPaused { 
                                                    id: id.to_string(), 
                                                    name: name.clone() 
                                                });
                                                is_paused = true;
                                                // Continue waiting - don't execute task yet
                                                continue;
                                            }
                                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                                warn!(missed_messages = n, "Control channel lagged during cron wait, messages skipped");
                                            }
                                            Err(broadcast::error::RecvError::Closed) => {
                                                warn!("Control channel closed during cron wait");
                                            }
                                            _ => {
                                                // Other messages don't interrupt cron waiting
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, schedule = schedule_str, "Invalid cron expression");
                    }
                }
                
                // Skip normal backoff for cron tasks - they follow their schedule
                continue;
            }

            // Check for any pending control messages
            while let Ok(msg) = control_rx.try_recv() {
                match msg {
                    ControlMessage::Stop => {
                        info!("Received Stop command");
                        let _ = event_tx.send(SupervisorEvent::TaskStopped { id: id.to_string(), name: name.clone() });
                        task.cleanup().await;
                        return SupervisionResult {
                            task_name: name,
                            task_id: id.to_string(),
                            total_attempts: attempt,
                            final_status: SupervisionStatus::ManuallyStopped,
                        };
                    }
                    ControlMessage::Pause => {
                        info!("Received Pause command");
                        let _ = event_tx.send(SupervisorEvent::TaskPaused { id: id.to_string(), name: name.clone() });
                        is_paused = true;
                    }
                    ControlMessage::Resume => {
                        info!("Received Resume command");
                        let _ = event_tx.send(SupervisorEvent::TaskResumed { id: id.to_string(), name: name.clone() });
                        is_paused = false;
                    }
                    ControlMessage::Restart | ControlMessage::ResetCircuitBreaker => {}
                }
            }

            // Calculate backoff delay with rate limiting
            // Calculate backoff delay using hot-reloaded config if available
            let backoff_strategy = if let Some(config_lock) = &task_config {
                let config = config_lock.read().await;
                config.backoff_strategy.clone()
            } else {
                task.backoff_strategy()
            };
            
            let mut delay = backoff_strategy.calculate_delay(attempt);
            
            // Enforce minimum restart interval to prevent resource exhaustion
            if let Some(min_interval) = task.min_restart_interval()
                && delay < min_interval {
                info!(
                    backoff_ms = delay.as_millis(),
                    min_interval_ms = min_interval.as_millis(),
                    "Rate limiting: extending delay to minimum restart interval"
                );
                delay = min_interval;
            }
            
            warn!(delay_ms = delay.as_millis(), attempt, "Scheduling restart after backoff");
            let _ = event_tx.send(SupervisorEvent::TaskBackoff { 
                id: id.to_string(), 
                name: name.clone(), 
                attempt, 
                delay 
            });

            // Sleep for backoff duration, but remain responsive to control messages
            tokio::time::sleep(delay).await;

            // Check for any pending control messages after sleep
            while let Ok(msg) = control_rx.try_recv() {
                match msg {
                    ControlMessage::Stop => {
                        info!("Received Stop command during backoff");
                        let _ = event_tx.send(SupervisorEvent::TaskStopped { id: id.to_string(), name: name.clone() });
                        task.cleanup().await;
                        return SupervisionResult {
                            task_name: name,
                            task_id: id.to_string(),
                            total_attempts: attempt,
                            final_status: SupervisionStatus::ManuallyStopped,
                        };
                    }
                    ControlMessage::Restart => {
                        info!("Received Restart command during backoff, bypassing delay");
                    }
                    ControlMessage::Pause | ControlMessage::Resume | ControlMessage::ResetCircuitBreaker => {
                        // Ignore these during backoff
                    }
                }
            }
        }

        task.cleanup().await;
        SupervisionResult {
            task_name: name,
            task_id: id.to_string(),
            total_attempts: attempt,
            final_status: SupervisionStatus::ManuallyStopped,
        }
    }.instrument(supervision_span))
}

/// Waits for a dependency to signal completion or failure
async fn wait_for_dependency(
    dep_id: &'static str,
    rx: &mut watch::Receiver<Option<Result<(), String>>>,
    _task_id: &str,
) -> Result<(), String> {
    info!(dependency = dep_id, "Waiting for dependency setup");
    
    // Wait for the dependency to signal completion
    // watch::changed() waits for a new value, but we also need to check current value
    loop {
        // Check if value is already set (handles race condition)
        if let Some(result) = rx.borrow().clone() {
            return match result {
                Ok(()) => Ok(()),
                Err(e) => Err(format!("Dependency '{dep_id}' failed: {e}")),
            };
        }
        
        // Wait for a change
        if rx.changed().await.is_err() {
            return Err(format!("Dependency '{dep_id}' channel closed unexpectedly"));
        }
        // Loop continues to check the new value
    }
}

/// Process pending control messages and return action if task should stop
#[allow(clippy::too_many_arguments)]
async fn process_control_messages(
    control_rx: &mut broadcast::Receiver<ControlMessage>,
    event_tx: &broadcast::Sender<SupervisorEvent>,
    task_id: &str,
    task_name: &str,
    task: &Arc<dyn SupervisedTask>,
    is_paused: &mut bool,
    circuit_breaker: &mut Option<CircuitBreaker>,
    attempt: usize,
) -> Option<SupervisionResult> {
    while let Ok(msg) = control_rx.try_recv() {
        match msg {
            ControlMessage::Stop => {
                info!("Received Stop command");
                let _ = event_tx.send(SupervisorEvent::TaskStopped { 
                    id: task_id.to_string(), 
                    name: task_name.to_string() 
                });
                task.cleanup().await;
                return Some(SupervisionResult {
                    task_name: task_name.to_string(),
                    task_id: task_id.to_string(),
                    total_attempts: attempt,
                    final_status: SupervisionStatus::ManuallyStopped,
                });
            }
            ControlMessage::Pause => {
                info!("Received Pause command");
                let _ = event_tx.send(SupervisorEvent::TaskPaused { 
                    id: task_id.to_string(), 
                    name: task_name.to_string() 
                });
                *is_paused = true;
            }
            ControlMessage::Resume => {
                info!("Received Resume command");
                let _ = event_tx.send(SupervisorEvent::TaskResumed { 
                    id: task_id.to_string(), 
                    name: task_name.to_string() 
                });
                *is_paused = false;
            }
            ControlMessage::Restart => {
                info!("Received Restart command");
            }
            ControlMessage::ResetCircuitBreaker => {
                if let Some(cb) = circuit_breaker {
                    cb.reset();
                }
            }
        }
    }
    None
}

/// Action to take after handling a task result
enum TaskResultAction {
    /// Complete the supervision loop with the given status
    Complete(SupervisionResult),
    /// Continue to the next iteration (backoff and retry)
    Continue,
    /// Restart was prevented by should_restart hook
    RestartPrevented,
}

/// Handle the result of a task execution
#[allow(clippy::too_many_arguments)]
async fn handle_task_result(
    result: Result<Result<(), anyhow::Error>, tokio::task::JoinError>,
    event_tx: &broadcast::Sender<SupervisorEvent>,
    task_id: &str,
    task_name: &str,
    task: &Arc<dyn SupervisedTask>,
    attempt: usize,
    failure_count: &mut usize,
    last_success_timestamp_secs: &mut Option<u64>,
    circuit_breaker: &mut Option<CircuitBreaker>,
) -> TaskResultAction {
    match result {
        // Task completed successfully
        Ok(Ok(())) => {
            info!("Task completed successfully");
            
            // Update success timestamp
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            *last_success_timestamp_secs = Some(now);
            
            // Record success in circuit breaker
            if let Some(cb) = circuit_breaker {
                cb.record_success();
            }
            
            let _ = event_tx.send(SupervisorEvent::TaskFinished { 
                id: task_id.to_string(), 
                name: task_name.to_string(), 
                attempt 
            });
            
            task.cleanup().await;
            TaskResultAction::Complete(SupervisionResult {
                task_name: task_name.to_string(),
                task_id: task_id.to_string(),
                total_attempts: attempt,
                final_status: SupervisionStatus::CompletedNormally,
            })
        }
        
        // Task returned an error
        Ok(Err(e)) => {
            let error_msg = format!("{e:?}");
            error!(error = %error_msg, "Task execution failed");
            *failure_count += 1;
            
            // Record failure in circuit breaker
            if let Some(cb) = circuit_breaker {
                cb.record_failure();
            }
            
            let _ = event_tx.send(SupervisorEvent::TaskFailed { 
                id: task_id.to_string(), 
                name: task_name.to_string(), 
                attempt, 
                error: error_msg.clone() 
            });
            
            // Call error hook
            task.on_error(&error_msg, attempt).await;
            
            // Check if we should restart
            if !task.should_restart(attempt, &error_msg).await {
                warn!("Restart prevented by should_restart hook");
                let _ = event_tx.send(SupervisorEvent::TaskRestartPrevented { 
                    id: task_id.to_string(), 
                    name: task_name.to_string(), 
                    attempt 
                });
                TaskResultAction::RestartPrevented
            } else {
                TaskResultAction::Continue
            }
        }
        
        // Task panicked or was cancelled
        Err(join_err) => {
            let panic_msg = if join_err.is_panic() {
                format!("Task panicked: {join_err:?}")
            } else {
                "Task was cancelled".to_string()
            };
            
            error!(error = %panic_msg, "Task execution failed");
            *failure_count += 1;
            
            // Record failure in circuit breaker
            if let Some(cb) = circuit_breaker {
                cb.record_failure();
            }
            
            let _ = event_tx.send(SupervisorEvent::TaskPanicked { 
                id: task_id.to_string(), 
                name: task_name.to_string(), 
                attempt, 
                panic_info: panic_msg.clone() 
            });
            
            // Call panic hook
            task.on_panic(&panic_msg, attempt).await;
            
            // Check if we should restart
            if !task.should_restart(attempt, &panic_msg).await {
                warn!("Restart prevented by should_restart hook");
                let _ = event_tx.send(SupervisorEvent::TaskRestartPrevented { 
                    id: task_id.to_string(), 
                    name: task_name.to_string(), 
                    attempt 
                });
                TaskResultAction::RestartPrevented
            } else {
                TaskResultAction::Continue
            }
        }
    }
}

/// Handle a control message received during task execution
#[allow(clippy::too_many_arguments)]
async fn handle_control_message_during_execution(
    msg: Result<ControlMessage, broadcast::error::RecvError>,
    event_tx: &broadcast::Sender<SupervisorEvent>,
    task_id: &str,
    task_name: &str,
    task: &Arc<dyn SupervisedTask>,
    is_paused: &mut bool,
    circuit_breaker: &mut Option<CircuitBreaker>,
    run_handle: &mut tokio::task::JoinHandle<Result<(), anyhow::Error>>,
    attempt: usize,
) -> Option<SupervisionResult> {
    match msg {
        Ok(ControlMessage::Stop) => {
            info!("Received Stop command during execution");
            let _ = event_tx.send(SupervisorEvent::TaskStopped { 
                id: task_id.to_string(), 
                name: task_name.to_string() 
            });
            run_handle.abort();
            task.cleanup().await;
            Some(SupervisionResult {
                task_name: task_name.to_string(),
                task_id: task_id.to_string(),
                total_attempts: attempt,
                final_status: SupervisionStatus::ManuallyStopped,
            })
        }
        Ok(ControlMessage::Restart) => {
            info!("Received Restart command during execution");
            run_handle.abort();
            None // Will trigger restart by breaking with None
        }
        Ok(ControlMessage::Pause) => {
            info!("Received Pause command during execution");
            let _ = event_tx.send(SupervisorEvent::TaskPaused { 
                id: task_id.to_string(), 
                name: task_name.to_string() 
            });
            *is_paused = true;
            None
        }
        Ok(ControlMessage::Resume) => {
            info!("Received Resume command");
            let _ = event_tx.send(SupervisorEvent::TaskResumed { 
                id: task_id.to_string(), 
                name: task_name.to_string() 
            });
            *is_paused = false;
            None
        }
        Ok(ControlMessage::ResetCircuitBreaker) => {
            if let Some(cb) = circuit_breaker {
                cb.reset();
            }
            None
        }
        Err(broadcast::error::RecvError::Lagged(n)) => {
            warn!(task_id, missed_messages = n, "Control channel lagged, messages skipped");
            None
        }
        Err(broadcast::error::RecvError::Closed) => {
            warn!(task_id, "Control channel closed");
            None
        }
    }
}
