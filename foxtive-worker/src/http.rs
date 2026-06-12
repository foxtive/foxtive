use crate::health::{HealthCheck, HealthStatus};
use std::sync::Arc;

/// A wrapper to expose worker pool health via an HTTP endpoint.
///
/// This struct implements traits for common web frameworks to make
/// integrating health checks into your application straightforward.
pub struct HealthEndpoint {
    pub(crate) check: Arc<dyn HealthCheck>,
}

impl HealthEndpoint {
    /// Create a new health endpoint from a health-checkable component.
    pub fn new(check: impl HealthCheck + 'static) -> Self {
        Self {
            check: Arc::new(check),
        }
    }

    /// Get the current health status as a JSON-serializable string.
    pub fn get_status_json(&self) -> String {
        let status = self.check.check_health();
        let message = self.check.status_message();

        let status_str = match status {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Degraded { .. } => "degraded",
            HealthStatus::Unhealthy { .. } => "unhealthy",
        };

        format!(
            "{{\"status\":\"{}\",\"message\":\"{}\"}}",
            status_str,
            message.replace('"', "\\\"")
        )
    }

    /// Get the HTTP status code corresponding to the current health.
    pub fn get_http_status_code(&self) -> u16 {
        match self.check.check_health() {
            HealthStatus::Healthy => 200,
            HealthStatus::Degraded { .. } => 200, // Often degraded is still 200 but with info
            HealthStatus::Unhealthy { .. } => 503,
        }
    }
}

#[cfg(feature = "http")]
mod axum_integration {
    use super::*;
    use axum::{Json, http::StatusCode, response::IntoResponse};
    use serde::Serialize;

    #[derive(Serialize)]
    struct HealthResponse {
        status: String,
        message: String,
    }

    impl IntoResponse for HealthEndpoint {
        fn into_response(self) -> axum::response::Response {
            let status = self.check.check_health();
            let message = self.check.status_message();

            let (code, status_str) = match status {
                HealthStatus::Healthy => (StatusCode::OK, "healthy"),
                HealthStatus::Degraded { .. } => (StatusCode::OK, "degraded"),
                HealthStatus::Unhealthy { .. } => (StatusCode::SERVICE_UNAVAILABLE, "unhealthy"),
            };

            let body = Json(HealthResponse {
                status: status_str.to_string(),
                message,
            });

            (code, body).into_response()
        }
    }
}
