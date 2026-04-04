//! Implements the Circuit Breaker pattern for supervised tasks.

use crate::enums::{CircuitBreakerConfig, SupervisorEvent};
use std::time::Instant;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// Represents the state of a circuit breaker.
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState {
    /// The circuit is closed, allowing requests to pass through.
    Closed,
    /// The circuit is open, blocking requests and failing fast.
    Open { opened_at: Instant },
    /// The circuit is half-open, allowing a single test request to pass through.
    HalfOpen,
}

/// Implements the Circuit Breaker pattern for a supervised task.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: CircuitState,
    consecutive_failures: usize,
    event_tx: broadcast::Sender<SupervisorEvent>,
    task_id: String,
    task_name: String,
}

impl CircuitBreaker {
    /// Creates a new `CircuitBreaker` instance.
    pub fn new(
        config: CircuitBreakerConfig,
        event_tx: broadcast::Sender<SupervisorEvent>,
        task_id: String,
        task_name: String,
    ) -> Self {
        Self {
            config,
            state: CircuitState::Closed,
            consecutive_failures: 0,
            event_tx,
            task_id,
            task_name,
        }
    }

    /// Checks if the circuit is allowing execution.
    ///
    /// Returns `true` if the task can proceed, `false` otherwise.
    pub fn allow_request(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open { opened_at } => {
                if opened_at.elapsed() >= self.config.reset_timeout {
                    info!(
                        task_id = %self.task_id,
                        "Circuit breaker for task {} transitioning to Half-Open.",
                        self.task_id
                    );
                    self.state = CircuitState::HalfOpen;
                    let _ = self.event_tx.send(SupervisorEvent::CircuitBreakerHalfOpen {
                        id: self.task_id.clone(),
                        name: self.task_name.clone(),
                    });
                    true // Allow one request in Half-Open state
                } else {
                    false // Still in Open state, block request
                }
            }
            CircuitState::HalfOpen => true, // Allow one request
        }
    }

    /// Records a successful execution.
    ///
    /// If the circuit was Half-Open, it transitions back to Closed.
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        if self.state != CircuitState::Closed {
            info!(
                task_id = %self.task_id,
                "Circuit breaker for task {} transitioning to Closed (success).",
                self.task_id
            );
            self.state = CircuitState::Closed;
            let _ = self.event_tx.send(SupervisorEvent::CircuitBreakerReset {
                id: self.task_id.clone(),
                name: self.task_name.clone(),
            });
        }
    }

    /// Records a failed execution.
    ///
    /// If the circuit was Closed or Half-Open, it may transition to Open.
    pub fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        match self.state {
            CircuitState::Closed => {
                if self.consecutive_failures >= self.config.failure_threshold {
                    info!(
                        task_id = %self.task_id,
                        consecutive_failures = self.consecutive_failures,
                        "Circuit breaker for task {} tripping to Open.",
                        self.task_id
                    );
                    self.state = CircuitState::Open {
                        opened_at: Instant::now(),
                    };
                    let _ = self.event_tx.send(SupervisorEvent::CircuitBreakerTripped {
                        id: self.task_id.clone(),
                        name: self.task_name.clone(),
                        consecutive_failures: self.consecutive_failures,
                    });
                }
            }
            CircuitState::HalfOpen => {
                warn!(
                    task_id = %self.task_id,
                    "Circuit breaker for task {} failed in Half-Open, returning to Open.",
                    self.task_id
                );
                self.state = CircuitState::Open {
                    opened_at: Instant::now(),
                };
                let _ = self.event_tx.send(SupervisorEvent::CircuitBreakerTripped {
                    id: self.task_id.clone(),
                    name: self.task_name.clone(),
                    consecutive_failures: self.consecutive_failures,
                });
            }
            CircuitState::Open { .. } => {
                // Already open, do nothing but increment failures
            }
        }
    }

    /// Resets the circuit breaker to the Closed state.
    pub fn reset(&mut self) {
        if self.state != CircuitState::Closed {
            info!(
                task_id = %self.task_id,
                "Circuit breaker for task {} manually reset to Closed.",
                self.task_id
            );
            self.state = CircuitState::Closed;
            self.consecutive_failures = 0;
            let _ = self.event_tx.send(SupervisorEvent::CircuitBreakerReset {
                id: self.task_id.clone(),
                name: self.task_name.clone(),
            });
        }
    }

    /// Returns the current state of the circuit breaker.
    pub fn state(&self) -> &CircuitState {
        &self.state
    }

    /// Returns the configured reset timeout for the circuit breaker.
    pub fn reset_timeout(&self) -> std::time::Duration {
        self.config.reset_timeout
    }
}
