mod common;

use foxtive::metrics::{InfraEvent, MetricsSink};
use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[test]
fn infra_event_health_check_is_debug() {
    let event = InfraEvent::HealthCheckCompleted {
        name: Cow::from("database"),
        duration: Duration::from_millis(42),
        healthy: true,
    };
    let debug = format!("{event:?}");
    assert!(debug.contains("database"));
    assert!(debug.contains("42"));
}

#[test]
fn infra_event_pool_stats_is_debug() {
    let event = InfraEvent::PoolStats {
        pool_name: Cow::from("redis"),
        available: 5,
        in_use: 3,
    };
    let debug = format!("{event:?}");
    assert!(debug.contains("redis"));
    assert!(debug.contains("5"));
    assert!(debug.contains("3"));
}

#[test]
fn infra_event_operation_completed_is_debug() {
    let event = InfraEvent::OperationCompleted {
        operation: Cow::from("user_login"),
        duration: Duration::from_millis(150),
        success: true,
    };
    let debug = format!("{event:?}");
    assert!(debug.contains("user_login"));
    assert!(debug.contains("150"));
}

#[test]
fn infra_event_health_report_generated_is_debug() {
    let event = InfraEvent::HealthReportGenerated {
        duration: Duration::from_millis(200),
        healthy: true,
        check_count: 5,
    };
    let debug = format!("{event:?}");
    assert!(debug.contains("200"));
    assert!(debug.contains("5"));
}

struct CountingSink {
    count: AtomicUsize,
}

impl CountingSink {
    fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
        }
    }

    fn event_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

impl MetricsSink for CountingSink {
    fn record(&self, _event: &InfraEvent) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn metrics_sink_receives_events_via_trait_object() {
    let sink = Arc::new(CountingSink::new());
    let sink_trait: Arc<dyn MetricsSink> = sink.clone();

    sink_trait.record(&InfraEvent::HealthCheckCompleted {
        name: Cow::from("test"),
        duration: Duration::from_millis(1),
        healthy: true,
    });
    sink_trait.record(&InfraEvent::PoolStats {
        pool_name: Cow::from("test"),
        available: 1,
        in_use: 0,
    });
    sink_trait.record(&InfraEvent::OperationCompleted {
        operation: Cow::from("test"),
        duration: Duration::from_millis(5),
        success: true,
    });

    assert_eq!(sink.event_count(), 3);
}

#[test]
fn metrics_sink_starts_at_zero() {
    let sink = CountingSink::new();
    assert_eq!(sink.event_count(), 0);
}
