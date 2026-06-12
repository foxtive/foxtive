use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use chrono::{DateTime, Utc};

/// Represents the health status of a component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    /// The component is operating normally.
    Healthy,
    /// The component is operating with some issues, but is still functional.
    Degraded { reason: String },
    /// The component is not operating correctly.
    Unhealthy { reason: String },
}

impl fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "Healthy"),
            HealthStatus::Degraded { reason } => write!(f, "Degraded: {}", reason),
            HealthStatus::Unhealthy { reason } => write!(f, "Unhealthy: {}", reason),
        }
    }
}

/// Provides a common interface for components to report their health.
pub trait HealthCheck: Send + Sync {
    /// Performs a health check and returns the current status.
    fn check_health(&self) -> HealthStatus;

    /// Returns a descriptive message about the health status.
    fn status_message(&self) -> String;
}

// Blanket implementation for Arc<T> where T: HealthCheck
impl<T: HealthCheck + 'static> HealthCheck for std::sync::Arc<T> {
    fn check_health(&self) -> HealthStatus {
        self.as_ref().check_health()
    }

    fn status_message(&self) -> String {
        self.as_ref().status_message()
    }
}

/// Tracks health metrics for an individual worker.
pub struct WorkerHealth {
    /// Worker identifier
    pub worker_id: String,
    /// Current health status
    pub status: Mutex<HealthStatus>,
    /// Timestamp of last health check
    pub last_check: Mutex<DateTime<Utc>>,
    /// Total messages processed successfully
    pub messages_processed: AtomicU64,
    /// Total messages failed
    pub messages_failed: AtomicU64,
    /// Average processing time (in milliseconds)
    pub avg_processing_time_ms: AtomicU64,
    /// Number of consecutive failures
    pub consecutive_failures: AtomicU64,
    /// Last error message (if any)
    pub last_error: Mutex<Option<String>>,
}

impl WorkerHealth {
    /// Create a new worker health tracker.
    pub fn new(worker_id: impl Into<String>) -> Self {
        Self {
            worker_id: worker_id.into(),
            status: Mutex::new(HealthStatus::Healthy),
            last_check: Mutex::new(Utc::now()),
            messages_processed: AtomicU64::new(0),
            messages_failed: AtomicU64::new(0),
            avg_processing_time_ms: AtomicU64::new(0),
            consecutive_failures: AtomicU64::new(0),
            last_error: Mutex::new(None),
        }
    }

    /// Record a successful message processing.
    pub fn record_success(&self, processing_time: Duration) {
        self.messages_processed.fetch_add(1, Ordering::Relaxed);
        self.consecutive_failures.store(0, Ordering::Relaxed);
        
        // Update average processing time (simple moving average)
        let current_avg = self.avg_processing_time_ms.load(Ordering::Relaxed);
        let new_avg = if current_avg == 0 {
            processing_time.as_millis() as u64
        } else {
            (current_avg + processing_time.as_millis() as u64) / 2
        };
        self.avg_processing_time_ms.store(new_avg, Ordering::Relaxed);
        
        self.update_status();
    }

    /// Record a failed message processing.
    pub fn record_failure(&self, error: impl Into<String>) {
        self.messages_failed.fetch_add(1, Ordering::Relaxed);
        self.consecutive_failures.fetch_add(1, Ordering::Relaxed);
        *self.last_error.lock().unwrap() = Some(error.into());
        self.update_status();
    }

    /// Update health status based on metrics.
    fn update_status(&self) {
        let consecutive_failures = self.consecutive_failures.load(Ordering::Relaxed);
        let total_messages = self.messages_processed.load(Ordering::Relaxed) 
            + self.messages_failed.load(Ordering::Relaxed);
        
        let new_status = if total_messages == 0 {
            HealthStatus::Healthy
        } else if consecutive_failures >= 10 {
            // Too many consecutive failures
            HealthStatus::Unhealthy { 
                reason: format!("{} consecutive failures", consecutive_failures) 
            }
        } else if consecutive_failures >= 3 {
            // Some consecutive failures
            HealthStatus::Degraded { 
                reason: format!("{} consecutive failures", consecutive_failures) 
            }
        } else {
            // Calculate failure rate
            let failure_rate = self.messages_failed.load(Ordering::Relaxed) as f64 / total_messages as f64;
            if failure_rate > 0.5 {
                HealthStatus::Degraded { 
                    reason: format!("High failure rate: {:.1}%", failure_rate * 100.0) 
                }
            } else {
                HealthStatus::Healthy
            }
        };
        
        *self.status.lock().unwrap() = new_status;
        *self.last_check.lock().unwrap() = Utc::now();
    }

    /// Get current health status.
    pub fn get_status(&self) -> HealthStatus {
        self.status.lock().unwrap().clone()
    }
}

impl fmt::Debug for WorkerHealth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorkerHealth")
            .field("worker_id", &self.worker_id)
            .field("status", &self.status)
            .field("messages_processed", &self.messages_processed.load(Ordering::Relaxed))
            .field("messages_failed", &self.messages_failed.load(Ordering::Relaxed))
            .field("avg_processing_time_ms", &self.avg_processing_time_ms.load(Ordering::Relaxed))
            .field("consecutive_failures", &self.consecutive_failures.load(Ordering::Relaxed))
            .finish()
    }
}

/// A basic health check implementation for the WorkerPool.
pub struct WorkerPoolHealth {
    pool_name: String,
    worker_count: usize,
    // In a real scenario, this would track more granular health, e.g.,
    // worker error rates, backend connection status, etc.
    is_running: bool,
}

impl WorkerPoolHealth {
    pub fn new(pool_name: String, worker_count: usize, is_running: bool) -> Self {
        Self {
            pool_name,
            worker_count,
            is_running,
        }
    }
}

impl HealthCheck for WorkerPoolHealth {
    fn check_health(&self) -> HealthStatus {
        if !self.is_running {
            HealthStatus::Unhealthy { 
                reason: "Pool is not running".to_string() 
            }
        } else if self.worker_count == 0 {
            HealthStatus::Degraded { 
                reason: "No workers available".to_string() 
            }
        } else {
            HealthStatus::Healthy
        }
    }

    fn status_message(&self) -> String {
        match self.check_health() {
            HealthStatus::Healthy => {
                format!("WorkerPool '{}' is healthy with {} workers.", self.pool_name, self.worker_count)
            }
            HealthStatus::Degraded { reason } => {
                format!("WorkerPool '{}' is degraded: {}. {} workers running.", 
                    self.pool_name, reason, self.worker_count)
            }
            HealthStatus::Unhealthy { reason } => {
                format!("WorkerPool '{}' is unhealthy: {}.", self.pool_name, reason)
            }
        }
    }
}
