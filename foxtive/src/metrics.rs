//! Infrastructure metrics abstraction.
//!
//! Provides a [`MetricsSink`] trait for recording infrastructure events
//! (pool stats, operation latency, etc.) and an [`InfraEvent`] enum that
//! categorises the kinds of events the framework emits.
//!
//! Companion crates or application code can implement [`MetricsSink`] to
//! forward events to Prometheus, OpenTelemetry, or any other backend.

use std::borrow::Cow;
use std::time::Duration;

/// Events emitted by the framework's infrastructure layer.
#[derive(Debug, Clone)]
pub enum InfraEvent {
    /// A health check completed (includes per-check duration).
    HealthCheckCompleted {
        name: Cow<'static, str>,
        duration: Duration,
        healthy: bool,
    },

    /// A full health report was generated.
    HealthReportGenerated {
        duration: Duration,
        healthy: bool,
        check_count: usize,
    },

    /// Connection pool statistics snapshot.
    PoolStats {
        pool_name: Cow<'static, str>,
        available: usize,
        in_use: usize,
    },

    /// A generic infrastructure operation completed.
    OperationCompleted {
        operation: Cow<'static, str>,
        duration: Duration,
        success: bool,
    },
}

/// Trait for receiving infrastructure metrics events.
///
/// Implement this trait to forward events to your metrics backend
/// (Prometheus, OpenTelemetry, StatsD, etc.).
///
/// # Example
///
/// ```
/// use foxtive::metrics::{MetricsSink, InfraEvent};
///
/// struct LoggingSink;
///
/// impl MetricsSink for LoggingSink {
///     fn record(&self, event: &InfraEvent) {
///         println!("{event:?}");
///     }
/// }
/// ```
pub trait MetricsSink: Send + Sync + 'static {
    /// Record an infrastructure metrics event.
    fn record(&self, event: &InfraEvent);
}
