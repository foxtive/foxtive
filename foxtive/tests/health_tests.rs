mod common;

use foxtive::health::{HealthReport, HealthStatus};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn healthy_status_is_healthy() {
    let status = HealthStatus::Healthy;
    assert!(status.is_healthy());
}

#[test]
fn degraded_constructor_sets_message() {
    let status = HealthStatus::degraded("slow response");
    assert!(!status.is_healthy());
    assert_eq!(status.to_string(), "degraded: slow response");
}

#[test]
fn unhealthy_constructor_sets_message() {
    let status = HealthStatus::unhealthy("connection refused");
    assert!(!status.is_healthy());
    assert_eq!(status.to_string(), "unhealthy: connection refused");
}

#[test]
fn health_report_all_healthy() {
    let report = HealthReport {
        status: HealthStatus::Healthy,
        checks: Arc::new(vec![
            ("db".into(), HealthStatus::Healthy),
            ("redis".into(), HealthStatus::Healthy),
            ("cache".into(), HealthStatus::Healthy),
        ]),
        duration: Duration::from_millis(15),
    };
    assert!(report.is_healthy());
    assert_eq!(report.checks.len(), 3);
}

#[test]
fn health_report_with_degraded_check() {
    let report = HealthReport {
        status: HealthStatus::degraded("some checks slow"),
        checks: Arc::new(vec![
            ("db".into(), HealthStatus::Healthy),
            ("redis".into(), HealthStatus::degraded("high latency")),
        ]),
        duration: Duration::from_millis(500),
    };
    assert!(!report.is_healthy());
}

#[test]
fn health_report_with_unhealthy_check() {
    let report = HealthReport {
        status: HealthStatus::unhealthy("critical failure"),
        checks: Arc::new(vec![
            ("db".into(), HealthStatus::unhealthy("connection refused")),
            ("redis".into(), HealthStatus::Healthy),
        ]),
        duration: Duration::from_millis(100),
    };
    assert!(!report.is_healthy());
    assert_eq!(report.checks.len(), 2);
}

#[test]
fn health_report_empty_checks() {
    let report = HealthReport {
        status: HealthStatus::Healthy,
        checks: Arc::new(vec![]),
        duration: Duration::from_millis(0),
    };
    assert!(report.is_healthy());
    assert_eq!(report.checks.len(), 0);
}
