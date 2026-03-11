#[cfg(feature = "reqwest")]
use crate::helpers::reqwest::ReqwestResponseError;
use crate::results::AppResult;
use http::StatusCode;
use std::borrow::Cow;
use std::env::VarError;
use std::fmt::{Debug, Display, Formatter};
use thiserror::Error;
use tracing::{error, info, warn};
use crate::ValidationErrors;

#[derive(Error, Debug, Clone)]
pub enum AppMessage {
    Success(String),
    Redirect(String),
    Invalid(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    UnprocessableEntity(String),
    ValidationError(String, ValidationErrors),
    InternalServerError(String),
    ErrorMessage(String, StatusCode),
    MissingEnvironmentVariable(String, VarError),
    #[cfg(feature = "reqwest")]
    ReqwestResponseError(ReqwestResponseError),
}

impl Display for AppMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl AppMessage {
    // Constructors

    /// Creates a new success message.
    pub fn success(msg: impl Into<String>) -> Self {
        AppMessage::Success(msg.into())
    }

    /// Creates a new redirect message.
    pub fn redirect(url: impl Into<String>) -> Self {
        AppMessage::Redirect(url.into())
    }

    /// Creates a new invalid request message (400 Bad Request).
    pub fn invalid(msg: impl Into<String>) -> Self {
        AppMessage::Invalid(msg.into())
    }

    /// Creates a new unauthorized message (401).
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        AppMessage::Unauthorized(msg.into())
    }

    /// Creates a new forbidden message (403).
    pub fn forbidden(msg: impl Into<String>) -> Self {
        AppMessage::Forbidden(msg.into())
    }

    /// Creates a new not found message (404).
    pub fn not_found(msg: impl Into<String>) -> Self {
        AppMessage::NotFound(msg.into())
    }

    /// Creates a new conflict message (409).
    pub fn conflict(msg: impl Into<String>) -> Self {
        AppMessage::Conflict(msg.into())
    }

    /// Creates a new unprocessable entity message (422).
    pub fn unprocessable_entity(msg: impl Into<String>) -> Self {
        AppMessage::UnprocessableEntity(msg.into())
    }

    /// Creates a validation error (422) with per-field error details.
    ///
    /// # Example
    /// ```
    /// use foxtive::enums::AppMessage;
    /// use foxtive::ValidationErrors;
    ///
    /// let mut errors = ValidationErrors::new();
    /// errors.insert("email".into(), vec!["is required".into()]);
    /// let msg = AppMessage::validation_error("Validation failed", errors);
    /// ```
    pub fn validation_error(
        msg: impl Into<String>,
        errors: impl Into<ValidationErrors>,
    ) -> Self {
        AppMessage::ValidationError(msg.into(), errors.into())
    }

    /// Creates a new internal server error message (500).
    pub fn internal_server_error(msg: impl Into<String>) -> Self {
        AppMessage::InternalServerError(msg.into())
    }

    /// Creates an error message with an explicit status code.
    pub fn error_message(msg: impl Into<String>, status: StatusCode) -> Self {
        AppMessage::ErrorMessage(msg.into(), status)
    }

    /// Creates a missing environment variable error (500).
    pub fn missing_environment_variable(name: impl Into<String>, error: VarError) -> Self {
        AppMessage::MissingEnvironmentVariable(name.into(), error)
    }

    #[cfg(feature = "reqwest")]
    /// Creates a Reqwest response error message.
    pub fn reqwest_response_error(err: ReqwestResponseError) -> Self {
        AppMessage::ReqwestResponseError(err)
    }

    // Accessors

    /// Returns the HTTP status code associated with this message.
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppMessage::Success(_) => StatusCode::OK,
            AppMessage::Redirect(_) => StatusCode::FOUND,
            AppMessage::Invalid(_) => StatusCode::BAD_REQUEST,
            AppMessage::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppMessage::Forbidden(_) => StatusCode::FORBIDDEN,
            AppMessage::NotFound(_) => StatusCode::NOT_FOUND,
            AppMessage::Conflict(_) => StatusCode::CONFLICT,
            AppMessage::UnprocessableEntity(_) | AppMessage::ValidationError(_, _) => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            AppMessage::InternalServerError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppMessage::MissingEnvironmentVariable(_, _) => StatusCode::INTERNAL_SERVER_ERROR,
            AppMessage::ErrorMessage(_, status) => *status,
            #[cfg(feature = "reqwest")]
            AppMessage::ReqwestResponseError(err) => *err.code(),
        }
    }

    /// Returns the human-readable message text.
    pub fn message(&self) -> Cow<'_, str> {
        match self {
            AppMessage::Success(msg)
            | AppMessage::Invalid(msg)
            | AppMessage::Redirect(msg)
            | AppMessage::Unauthorized(msg)
            | AppMessage::Forbidden(msg)
            | AppMessage::NotFound(msg)
            | AppMessage::Conflict(msg)
            | AppMessage::UnprocessableEntity(msg)
            | AppMessage::InternalServerError(msg) => Cow::from(msg),
            AppMessage::ValidationError(msg, _) => Cow::from(msg),
            AppMessage::ErrorMessage(msg, _) => Cow::from(msg),
            AppMessage::MissingEnvironmentVariable(name, e) => {
                Cow::from(format!("Missing environment variable '{name}': {e}"))
            }
            #[cfg(feature = "reqwest")]
            AppMessage::ReqwestResponseError(err) => Cow::from(err.body().to_string()),
        }
    }

    /// Returns field-level validation errors, if this is a `ValidationError`.
    pub fn validation_errors(&self) -> Option<&ValidationErrors> {
        match self {
            AppMessage::ValidationError(_, errors) => Some(errors),
            _ => None,
        }
    }

    /// Returns a stable string identifier for the variant (useful for logging/tracing).
    pub fn kind_name(&self) -> &'static str {
        match self {
            AppMessage::Success(_) => "success",
            AppMessage::Redirect(_) => "redirect",
            AppMessage::Invalid(_) => "invalid",
            AppMessage::Unauthorized(_) => "unauthorized",
            AppMessage::Forbidden(_) => "forbidden",
            AppMessage::NotFound(_) => "not_found",
            AppMessage::Conflict(_) => "conflict",
            AppMessage::UnprocessableEntity(_) => "unprocessable_entity",
            AppMessage::ValidationError(_, _) => "validation_error",
            AppMessage::InternalServerError(_) => "internal_server_error",
            AppMessage::MissingEnvironmentVariable(_, _) => "missing_environment_variable",
            AppMessage::ErrorMessage(_, _) => "error_message",
            #[cfg(feature = "reqwest")]
            AppMessage::ReqwestResponseError(_) => "reqwest_response_error",
        }
    }

    // Status category helpers

    /// Returns `true` if the status code is 2xx.
    pub fn is_success(&self) -> bool {
        self.status_code().is_success()
    }

    /// Returns `true` if the status code is 3xx.
    pub fn is_redirect(&self) -> bool {
        self.status_code().is_redirection()
    }

    /// Returns `true` if the status code is 4xx.
    pub fn is_client_error(&self) -> bool {
        self.status_code().is_client_error()
    }

    /// Returns `true` if the status code is 5xx.
    pub fn is_server_error(&self) -> bool {
        self.status_code().is_server_error()
    }

    /// Returns `true` if the status code is 4xx or 5xx.
    pub fn is_error(&self) -> bool {
        self.is_client_error() || self.is_server_error()
    }

    // Observability

    /// Logs the message at the appropriate tracing level, including kind and status.
    pub fn log(&self) {
        let kind = self.kind_name();
        let status = self.status_code().as_u16();
        let msg = self.message();

        if self.is_success() || self.is_redirect() {
            info!(kind, status, "{}", msg);
        } else if self.is_client_error() {
            warn!(kind, status, "{}", msg);
        } else {
            error!(kind, status, "{}", msg);
        }
    }

    // Conversions

    /// Converts into an `anyhow::Error`.
    pub fn into_anyhow(self) -> anyhow::Error {
        anyhow::Error::from(self)
    }

    /// Converts into an `AppResult<T>` (always `Err`).
    pub fn into_result<T>(self) -> AppResult<T> {
        Err(self.into_anyhow())
    }
}

impl From<crate::Error> for AppMessage {
    fn from(value: anyhow::Error) -> Self {
        value.downcast::<AppMessage>().unwrap_or_else(|e| {
            error!("AppMessage downcast failed, wrapping as InternalServerError: {e}");
            AppMessage::InternalServerError(e.to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_success() {
        let msg = AppMessage::success("User created");
        assert_eq!(msg.status_code(), StatusCode::OK);
        assert_eq!(msg.message(), "User created");
        assert!(msg.is_success());
        assert!(!msg.is_error());
        assert!(!msg.is_redirect());
        assert_eq!(msg.kind_name(), "success");
    }

    #[test]
    fn test_redirect_is_not_an_error() {
        let msg = AppMessage::redirect("https://foxtive.com");
        assert_eq!(msg.status_code(), StatusCode::FOUND);
        assert!(msg.is_redirect());
        assert!(!msg.is_error()); // was broken before — redirects are not errors
        assert!(!msg.is_success());
        assert_eq!(msg.kind_name(), "redirect");
    }

    #[test]
    fn test_invalid() {
        let msg = AppMessage::invalid("Invalid pin");
        assert_eq!(msg.status_code(), StatusCode::BAD_REQUEST);
        assert!(msg.is_client_error());
        assert!(msg.is_error());
        assert_eq!(msg.kind_name(), "invalid");
    }

    #[test]
    fn test_unauthorized() {
        let msg = AppMessage::unauthorized("Invalid auth token");
        assert_eq!(msg.status_code(), StatusCode::UNAUTHORIZED);
        assert!(msg.is_client_error());
        assert_eq!(msg.message(), "Invalid auth token");
    }

    #[test]
    fn test_forbidden() {
        let msg = AppMessage::forbidden("Insufficient permissions");
        assert_eq!(msg.status_code(), StatusCode::FORBIDDEN);
        assert!(msg.is_client_error());
    }

    #[test]
    fn test_not_found() {
        let msg = AppMessage::not_found("Could not locate wallet");
        assert_eq!(msg.status_code(), StatusCode::NOT_FOUND);
        assert!(msg.is_client_error());
    }

    #[test]
    fn test_conflict() {
        let msg = AppMessage::conflict("Email already in use");
        assert_eq!(msg.status_code(), StatusCode::CONFLICT);
        assert!(msg.is_client_error());
        assert_eq!(msg.kind_name(), "conflict");
    }

    #[test]
    fn test_unprocessable_entity() {
        let msg = AppMessage::unprocessable_entity("Invalid payload");
        assert_eq!(msg.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(msg.is_client_error());
    }

    #[test]
    fn test_validation_error() {
        let mut errors = ValidationErrors::new();
        errors.insert("email".into(), vec!["is required".into()]);
        errors.insert("name".into(), vec!["is too short".into()]);

        let msg = AppMessage::validation_error("Validation failed", errors.clone());
        assert_eq!(msg.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(msg.is_client_error());
        assert_eq!(msg.message(), "Validation failed");
        assert_eq!(msg.kind_name(), "validation_error");

        let returned = msg.validation_errors().unwrap();
        assert_eq!(returned["email"], vec!["is required"]);
        assert_eq!(returned["name"], vec!["is too short"]);
    }

    #[test]
    fn test_validation_errors_none_for_other_variants() {
        assert!(AppMessage::not_found("x").validation_errors().is_none());
        assert!(AppMessage::success("x").validation_errors().is_none());
    }

    #[test]
    fn test_internal_server_error() {
        let msg = AppMessage::internal_server_error("Y2k ever!");
        assert_eq!(msg.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(msg.is_server_error());
        assert!(msg.is_error());
    }

    #[test]
    fn test_error_message_explicit_status() {
        let msg = AppMessage::error_message("Y2k huh?", StatusCode::BAD_REQUEST);
        assert_eq!(msg.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(msg.message(), "Y2k huh?");
    }

    #[test]
    fn test_missing_environment_variable() {
        let msg = AppMessage::missing_environment_variable("DATABASE_URL", env::VarError::NotPresent);
        assert_eq!(msg.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(msg.is_server_error());
        assert_eq!(
            msg.message(),
            "Missing environment variable 'DATABASE_URL': environment variable not found"
        );
    }

    #[test]
    fn test_into_result() {
        let result = AppMessage::error_message("Y2k huh?", StatusCode::BAD_REQUEST).into_result::<()>();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Y2k huh?");
    }

    #[test]
    fn test_into_anyhow() {
        let error = AppMessage::error_message("Y2k huh?", StatusCode::BAD_REQUEST).into_anyhow();
        assert_eq!(error.to_string(), "Y2k huh?");
    }

    #[test]
    fn test_kind_name_coverage() {
        assert_eq!(AppMessage::success("").kind_name(), "success");
        assert_eq!(AppMessage::redirect("").kind_name(), "redirect");
        assert_eq!(AppMessage::invalid("").kind_name(), "invalid");
        assert_eq!(AppMessage::unauthorized("").kind_name(), "unauthorized");
        assert_eq!(AppMessage::forbidden("").kind_name(), "forbidden");
        assert_eq!(AppMessage::not_found("").kind_name(), "not_found");
        assert_eq!(AppMessage::conflict("").kind_name(), "conflict");
        assert_eq!(AppMessage::unprocessable_entity("").kind_name(), "unprocessable_entity");
        assert_eq!(
            AppMessage::validation_error("", ValidationErrors::new()).kind_name(),
            "validation_error"
        );
        assert_eq!(AppMessage::internal_server_error("").kind_name(), "internal_server_error");
        assert_eq!(
            AppMessage::missing_environment_variable("X", env::VarError::NotPresent).kind_name(),
            "missing_environment_variable"
        );
    }

    #[cfg(feature = "reqwest")]
    #[test]
    fn test_reqwest_response_error() {
        let msg = AppMessage::reqwest_response_error(ReqwestResponseError::create(
            StatusCode::BAD_REQUEST,
            "Field 'user_id' is required".to_string(),
        ));
        assert_eq!(msg.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(msg.message(), "Field 'user_id' is required");
        assert!(msg.is_client_error());
    }
}