//! Health check abstraction for application components.
//!
//! # Health Check Caching
//!
//! Health checks can be expensive to run on every request. The [`HealthCheckCache`]
//! type provides a simple TTL-based cache to avoid running checks too frequently.
//! This is especially important for production deployments where load balancers
//! may poll the health endpoint every 100ms.

use crate::App;
use crate::metrics::{InfraEvent, MetricsSink};
#[cfg(feature = "database")]
use crate::tokio::Tokio;
use std::borrow::Cow;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// The result of a single health check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    /// The component is healthy and operating normally.
    Healthy,
    /// The component is degraded but still functional.
    Degraded { detail: String },
    /// The component is unhealthy and may not be functional.
    Unhealthy { detail: String },
}

impl HealthStatus {
    /// Creates a `Degraded` status with a detail message.
    pub fn degraded(detail: impl Into<String>) -> Self {
        HealthStatus::Degraded {
            detail: detail.into(),
        }
    }

    /// Creates an `Unhealthy` status with a detail message.
    pub fn unhealthy(detail: impl Into<String>) -> Self {
        HealthStatus::Unhealthy {
            detail: detail.into(),
        }
    }

    /// Returns `true` if the status is `Healthy`.
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }
}

impl fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded { detail } => write!(f, "degraded: {detail}"),
            HealthStatus::Unhealthy { detail } => write!(f, "unhealthy: {detail}"),
        }
    }
}

/// Trait for health check implementations.
///
/// Each health check targets a specific component (database, redis, etc.)
/// and reports its status.
///
/// # Example
///
/// ```no_run
/// use foxtive::health::{HealthCheck, HealthStatus};
/// use foxtive::App;
/// use std::future::Future;
/// use std::pin::Pin;
///
/// struct DatabaseHealthCheck;
///
/// impl HealthCheck for DatabaseHealthCheck {
///     fn name(&self) -> &str {
///         "database"
///     }
///
///     fn check<'a>(
///         &'a self,
///         app: &'a App,
///     ) -> Pin<Box<dyn Future<Output = HealthStatus> + Send + 'a>> {
///         Box::pin(async {
///             // Perform actual health check
///             HealthStatus::Healthy
///         })
///     }
/// }
/// ```
pub trait HealthCheck: Send + Sync {
    /// Name of this health check (e.g., "database", "redis").
    fn name(&self) -> &str;

    /// Perform the health check.
    fn check<'a>(&'a self, app: &'a App)
    -> Pin<Box<dyn Future<Output = HealthStatus> + Send + 'a>>;
}

/// Aggregated health report from all health checks.
#[derive(Debug, Clone)]
pub struct HealthReport {
    /// Overall status (worst of all checks).
    pub status: HealthStatus,
    /// Individual check results: (name, status).
    /// Wrapped in Arc to make cloning cheap (avoids deep Vec clone).
    pub checks: Arc<Vec<(String, HealthStatus)>>,
    /// Total time taken to run all checks.
    pub duration: Duration,
}

impl HealthReport {
    /// Returns `true` if all checks reported `Healthy`.
    pub fn is_healthy(&self) -> bool {
        self.status.is_healthy()
    }
}

impl fmt::Display for HealthReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Health Report ({:?})", self.duration)?;
        writeln!(f, "Overall: {}", self.status)?;
        for (name, status) in self.checks.iter() {
            writeln!(f, "  {name}: {status}")?;
        }
        Ok(())
    }
}

/// Compute the overall (worst) status from a list of individual statuses.
pub(crate) fn aggregate_status(checks: &[(String, HealthStatus)]) -> HealthStatus {
    let mut worst = HealthStatus::Healthy;
    for (_, status) in checks {
        match status {
            HealthStatus::Unhealthy { .. } => return status.clone(),
            HealthStatus::Degraded { .. } => worst = status.clone(),
            HealthStatus::Healthy => {}
        }
    }
    worst
}

/// A simple TTL cache for health check results.
///
/// Prevents running expensive health checks on every request. Returns cached
/// results if they are still within the TTL window.
///
/// # Thundering Herd Protection
///
/// When the cache expires, only one caller runs the health checks while others
/// wait for the result. This prevents a "thundering herd" scenario where many
/// concurrent requests all trigger expensive health checks simultaneously.
///
/// # Example
///
/// ```no_run
/// use foxtive::health::HealthCheckCache;
/// use std::time::Duration;
///
/// # async fn example(app: foxtive::App) {
/// let cache = HealthCheckCache::new(Duration::from_secs(10));
///
/// // First call runs checks, subsequent calls return cached results
/// let report1 = cache.get_or_run(&app).await;
/// let report2 = cache.get_or_run(&app).await; // Returns cached report
/// # }
/// ```
pub struct HealthCheckCache {
    ttl: Duration,
    cached: Arc<RwLock<Option<(HealthReport, Instant)>>>,
    /// Coalesces concurrent refresh attempts - only one caller runs checks.
    refresh_lock: Arc<tokio::sync::Mutex<()>>,
}

impl HealthCheckCache {
    /// Create a new cache with the given TTL.
    ///
    /// # Arguments
    /// * `ttl` - How long to cache health check results before re-running checks
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            cached: Arc::new(RwLock::new(None)),
            refresh_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Get cached health report, or run checks if cache is stale.
    ///
    /// If a cached report exists and is within the TTL window, returns it immediately.
    /// Otherwise, acquires a refresh lock to ensure only one caller runs the checks,
    /// while others wait for the result.
    pub async fn get_or_run(&self, app: &App) -> HealthReport {
        // Fast path: check if we have a valid cached report
        {
            let guard = self.cached.read().await;
            if let Some((report, timestamp)) = guard.as_ref()
                && timestamp.elapsed() < self.ttl
            {
                return report.clone();
            }
        }

        // Slow path: acquire refresh lock to coalesce concurrent refreshes
        let _refresh_guard = self.refresh_lock.lock().await;

        // Double-check after acquiring lock (another caller may have refreshed)
        {
            let guard = self.cached.read().await;
            if let Some((report, timestamp)) = guard.as_ref()
                && timestamp.elapsed() < self.ttl
            {
                return report.clone();
            }
        }

        // Run checks and update cache
        let report = app.check_health().await;
        let mut guard = self.cached.write().await;
        *guard = Some((report.clone(), Instant::now()));
        report
    }

    /// Force invalidate the cache, causing the next call to re-run checks.
    pub async fn invalidate(&self) {
        let mut guard = self.cached.write().await;
        *guard = None;
    }
}

/// Run all health checks concurrently, each wrapped in a timeout.
///
/// Returns the list of `(name, status)` pairs and the total elapsed duration.
///
/// # Rate Limiting
///
/// Health check endpoints should be rate-limited in production to prevent
/// excessive resource usage. Consider implementing rate limiting at the HTTP
/// layer (e.g., using a middleware) or caching health check results for a
/// short duration (e.g., 10-30 seconds) to avoid running checks on every request.
pub(crate) async fn run_health_checks(
    checks: &[Box<dyn HealthCheck>],
    app: &App,
    per_check_timeout: Duration,
    metrics: Option<&Arc<dyn MetricsSink>>,
) -> (Vec<(String, HealthStatus)>, Duration) {
    let start = Instant::now();

    let futures: Vec<_> = checks
        .iter()
        .map(|check| {
            let name = check.name().to_string();
            let fut = check.check(app);
            let timeout_dur = per_check_timeout;
            let metrics = metrics.cloned();
            async move {
                let check_start = Instant::now();
                let status = match tokio::time::timeout(timeout_dur, fut).await {
                    Ok(s) => s,
                    Err(_) => HealthStatus::Unhealthy {
                        detail: format!("Health check '{name}' timed out after {timeout_dur:?}"),
                    },
                };
                let check_duration = check_start.elapsed();

                if let Some(sink) = metrics {
                    sink.record(&InfraEvent::HealthCheckCompleted {
                        name: Cow::from(name.clone()),
                        duration: check_duration,
                        healthy: status.is_healthy(),
                    });
                }

                (name, status)
            }
        })
        .collect();

    let results = futures_util::future::join_all(futures).await;
    let duration = start.elapsed();
    (results, duration)
}

/// Health check for the database connection pool.
///
/// Verifies that a connection can be obtained from the pool.
///
/// # Example
/// ```no_run
/// use foxtive::health::DatabaseHealthCheck;
/// use foxtive::App;
///
/// # async fn example() {
/// let app = App::builder("my-app", "MYAPP")
///     .health_check(DatabaseHealthCheck::new())
///     .build()
///     .await
///     .unwrap();
/// # }
/// ```
#[cfg(feature = "database")]
pub struct DatabaseHealthCheck {
    metrics: Option<Arc<dyn MetricsSink>>,
    /// Timeout for the health check operation.
    timeout: Duration,
}

#[cfg(feature = "database")]
impl DatabaseHealthCheck {
    pub fn new() -> Self {
        Self {
            metrics: None,
            timeout: Duration::from_secs(5),
        }
    }

    pub fn with_metrics(metrics: Arc<dyn MetricsSink>) -> Self {
        Self {
            metrics: Some(metrics),
            timeout: Duration::from_secs(5),
        }
    }

    /// Set a custom timeout for the health check operation.
    ///
    /// Defaults to 5 seconds.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[cfg(feature = "database")]
impl Default for DatabaseHealthCheck {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "database")]
impl HealthCheck for DatabaseHealthCheck {
    fn name(&self) -> &str {
        "database"
    }

    fn check<'a>(
        &'a self,
        app: &'a App,
    ) -> Pin<Box<dyn Future<Output = HealthStatus> + Send + 'a>> {
        let metrics = self.metrics.clone();
        let timeout_dur = self.timeout;
        Box::pin(async move {
            let start = Instant::now();

            let pool = match app.db() {
                Ok(p) => p,
                Err(_) => {
                    return HealthStatus::Unhealthy {
                        detail: "Database pool not configured".to_string(),
                    };
                }
            };

            let pool_for_get = pool.clone();
            let tokio_handle = match app.require::<Tokio>() {
                Ok(t) => t,
                Err(_) => {
                    return HealthStatus::Unhealthy {
                        detail: "Tokio runtime not available in DI container".to_string(),
                    };
                }
            };
            // Use tokio::time::timeout to prevent indefinite blocking
            let status = match tokio::time::timeout(
                timeout_dur,
                tokio_handle.block(move || {
                    pool_for_get
                        .get()
                        .map(|_| HealthStatus::Healthy)
                        .map_err(|e| crate::enums::AppMessage::Infrastructure {
                            message: format!("Failed to get database connection: {e}"),
                            source: None,
                        })
                }),
            )
            .await
            {
                Ok(Ok(s)) => s,
                Ok(Err(e)) => HealthStatus::Unhealthy {
                    detail: e.to_string(),
                },
                Err(_) => HealthStatus::Unhealthy {
                    detail: format!("Database health check timed out after {timeout_dur:?}"),
                },
            };

            if let Some(sink) = metrics {
                let pool_status = pool.state();
                let total = pool_status.connections as usize;
                let idle = pool_status.idle_connections as usize;
                sink.record(&InfraEvent::PoolStats {
                    pool_name: Cow::from("database"),
                    available: idle,
                    in_use: total.saturating_sub(idle),
                });
                sink.record(&InfraEvent::OperationCompleted {
                    operation: Cow::from("database_health_check"),
                    duration: start.elapsed(),
                    success: status.is_healthy(),
                });
            }

            status
        })
    }
}

/// Health check for the async database connection pool.
///
/// Verifies that a connection can be obtained from the async pool.
/// Unlike [`DatabaseHealthCheck`], this does not require `spawn_blocking`
/// since the pool checkout is natively async.
#[cfg(feature = "database-async")]
pub struct AsyncDatabaseHealthCheck {
    metrics: Option<Arc<dyn MetricsSink>>,
    /// Timeout for the health check operation.
    timeout: Duration,
}

#[cfg(feature = "database-async")]
impl AsyncDatabaseHealthCheck {
    pub fn new() -> Self {
        Self {
            metrics: None,
            timeout: Duration::from_secs(5),
        }
    }

    pub fn with_metrics(metrics: Arc<dyn MetricsSink>) -> Self {
        Self {
            metrics: Some(metrics),
            timeout: Duration::from_secs(5),
        }
    }

    /// Set a custom timeout for the health check operation.
    ///
    /// Defaults to 5 seconds.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[cfg(feature = "database-async")]
impl Default for AsyncDatabaseHealthCheck {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "database-async")]
impl HealthCheck for AsyncDatabaseHealthCheck {
    fn name(&self) -> &str {
        "async_database"
    }

    fn check<'a>(
        &'a self,
        app: &'a App,
    ) -> Pin<Box<dyn Future<Output = HealthStatus> + Send + 'a>> {
        let metrics = self.metrics.clone();
        let timeout_dur = self.timeout;
        Box::pin(async move {
            let start = Instant::now();

            let pool = match app.async_db() {
                Ok(p) => p,
                Err(_) => {
                    return HealthStatus::Unhealthy {
                        detail: "Async database pool not configured".to_string(),
                    };
                }
            };

            // Natively async — no spawn_blocking needed
            let status = match tokio::time::timeout(timeout_dur, pool.get()).await {
                Ok(Ok(_conn)) => HealthStatus::Healthy,
                Ok(Err(e)) => HealthStatus::Unhealthy {
                    detail: format!("Failed to get async database connection: {e}"),
                },
                Err(_) => HealthStatus::Unhealthy {
                    detail: format!("Async database health check timed out after {timeout_dur:?}"),
                },
            };

            if let Some(sink) = metrics {
                let pool_status = pool.status();
                let total = pool_status.size;
                let available = pool_status.available;
                sink.record(&InfraEvent::PoolStats {
                    pool_name: Cow::from("async_database"),
                    available,
                    in_use: total.saturating_sub(available),
                });
                sink.record(&InfraEvent::OperationCompleted {
                    operation: Cow::from("async_database_health_check"),
                    duration: start.elapsed(),
                    success: status.is_healthy(),
                });
            }

            status
        })
    }
}

/// Health check for the Redis connection.
///
/// Verifies that Redis is reachable by sending a PING command.
///
/// # Example
/// ```no_run
/// use foxtive::health::RedisHealthCheck;
/// use foxtive::App;
///
/// # async fn example() {
/// let app = App::builder("my-app", "MYAPP")
///     .health_check(RedisHealthCheck::new())
///     .build()
///     .await
///     .unwrap();
/// # }
/// ```
#[cfg(feature = "redis")]
pub struct RedisHealthCheck {
    timeout: Duration,
}

#[cfg(feature = "redis")]
impl RedisHealthCheck {
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(5),
        }
    }

    /// Set a custom timeout for the health check operation.
    ///
    /// Defaults to 5 seconds.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[cfg(feature = "redis")]
impl Default for RedisHealthCheck {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "redis")]
impl HealthCheck for RedisHealthCheck {
    fn name(&self) -> &str {
        "redis"
    }

    fn check<'a>(
        &'a self,
        app: &'a App,
    ) -> Pin<Box<dyn Future<Output = HealthStatus> + Send + 'a>> {
        let timeout_dur = self.timeout;
        Box::pin(async move {
            let redis = match app.redis() {
                Ok(r) => r,
                Err(_) => {
                    return HealthStatus::Unhealthy {
                        detail: "Redis not configured".to_string(),
                    };
                }
            };
            match tokio::time::timeout(timeout_dur, redis.ping()).await {
                Ok(Ok(_)) => HealthStatus::Healthy,
                Ok(Err(e)) => HealthStatus::Unhealthy {
                    detail: format!("Redis ping failed: {e}"),
                },
                Err(_) => HealthStatus::Unhealthy {
                    detail: format!("Redis ping timed out after {timeout_dur:?}"),
                },
            }
        })
    }
}

/// Health check for the RabbitMQ connection.
///
/// Verifies that the RabbitMQ pool can provide a connection.
///
/// # Example
/// ```no_run
/// use foxtive::health::RabbitMqHealthCheck;
/// use foxtive::App;
///
/// # async fn example() {
/// let app = App::builder("my-app", "MYAPP")
///     .health_check(RabbitMqHealthCheck::new())
///     .build()
///     .await
///     .unwrap();
/// # }
/// ```
#[cfg(feature = "rabbitmq")]
pub struct RabbitMqHealthCheck {
    timeout: Duration,
}

#[cfg(feature = "rabbitmq")]
impl RabbitMqHealthCheck {
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(5),
        }
    }

    /// Set a custom timeout for the health check operation.
    ///
    /// Defaults to 5 seconds.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[cfg(feature = "rabbitmq")]
impl Default for RabbitMqHealthCheck {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "rabbitmq")]
impl HealthCheck for RabbitMqHealthCheck {
    fn name(&self) -> &str {
        "rabbitmq"
    }

    fn check<'a>(
        &'a self,
        app: &'a App,
    ) -> Pin<Box<dyn Future<Output = HealthStatus> + Send + 'a>> {
        let timeout_dur = self.timeout;
        Box::pin(async move {
            let pool = match app.rabbitmq_pool() {
                Ok(p) => p,
                Err(_) => {
                    return HealthStatus::Unhealthy {
                        detail: "RabbitMQ pool not configured".to_string(),
                    };
                }
            };
            match tokio::time::timeout(timeout_dur, pool.get()).await {
                Ok(Ok(_conn)) => HealthStatus::Healthy,
                Ok(Err(e)) => HealthStatus::Unhealthy {
                    detail: format!("Failed to get RabbitMQ connection: {e}"),
                },
                Err(_) => HealthStatus::Unhealthy {
                    detail: format!("RabbitMQ health check timed out after {timeout_dur:?}"),
                },
            }
        })
    }
}
