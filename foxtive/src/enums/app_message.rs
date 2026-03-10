#[cfg(feature = "reqwest")]
use crate::helpers::reqwest::ReqwestResponseError;
use crate::results::AppResult;
use http::StatusCode;
use std::borrow::Cow;
use std::env::VarError;
use std::fmt::{Debug, Display, Formatter};
use thiserror::Error;
use tracing::{error, info};

#[derive(Error, Debug, Clone)]
pub enum AppMessage {
    Success(String),
    Warning(String),
    Redirect(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
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
    /// Creates a new success message.
    pub fn success(msg: impl Into<String>) -> Self {
        AppMessage::Success(msg.into())
    }

    /// Creates a new warning message.
    pub fn warning(msg: impl Into<String>) -> Self {
        AppMessage::Warning(msg.into())
    }

    /// Creates a new redirect message.
    pub fn redirect(url: impl Into<String>) -> Self {
        AppMessage::Redirect(url.into())
    }

    /// Creates a new unauthorized message.
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        AppMessage::Unauthorized(msg.into())
    }

    /// Creates a new forbidden message.
    pub fn forbidden(msg: impl Into<String>) -> Self {
        AppMessage::Forbidden(msg.into())
    }

    /// Creates a new not found message.
    pub fn not_found(msg: impl Into<String>) -> Self {
        AppMessage::NotFound(msg.into())
    }

    /// Creates a new internal server error message.
    pub fn internal_server_error(msg: impl Into<String>) -> Self {
        AppMessage::InternalServerError(msg.into())
    }

    /// Creates a new error message with a specific status code.
    pub fn error_message(msg: impl Into<String>, status: StatusCode) -> Self {
        AppMessage::ErrorMessage(msg.into(), status)
    }

    /// Creates a new missing environment variable error message.
    pub fn missing_environment_variable(name: impl Into<String>, error: VarError) -> Self {
        AppMessage::MissingEnvironmentVariable(name.into(), error)
    }

    #[cfg(feature = "reqwest")]
    /// Creates a new Reqwest response error message.
    pub fn reqwest_response_error(err: ReqwestResponseError) -> Self {
        AppMessage::ReqwestResponseError(err)
    }

    /// Get the status code
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppMessage::Success(_) => StatusCode::OK,
            AppMessage::Warning(_) => StatusCode::BAD_REQUEST,
            AppMessage::ErrorMessage(_, status) => *status,
            AppMessage::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppMessage::Forbidden(_) => StatusCode::FORBIDDEN,
            AppMessage::NotFound(_) => StatusCode::NOT_FOUND,
            #[cfg(feature = "reqwest")]
            AppMessage::ReqwestResponseError(err) => *err.code(),
            AppMessage::Redirect(_) => StatusCode::FOUND,
            AppMessage::InternalServerError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppMessage::MissingEnvironmentVariable(_, _) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Get the message
    pub fn message(&self) -> Cow<'_, str> {
        match self {
            AppMessage::Success(msg)
            | AppMessage::Warning(msg)
            | AppMessage::Redirect(msg)
            | AppMessage::Unauthorized(msg)
            | AppMessage::Forbidden(msg)
            | AppMessage::NotFound(msg)
            | AppMessage::InternalServerError(msg) => Cow::from(msg),
            AppMessage::ErrorMessage(msg, _) => Cow::from(msg),
            AppMessage::MissingEnvironmentVariable(name, e) => {
                Cow::from(format!("Missing environment variable '{name}': {e}"))
            }
            #[cfg(feature = "reqwest")]
            AppMessage::ReqwestResponseError(err) => Cow::from(err.body().to_string()),
        }
    }

    /// Check if the message is a success
    pub fn is_success(&self) -> bool {
        matches!(self, AppMessage::Success(_))
    }

    /// Check if the message is an error
    pub fn is_error(&self) -> bool {
        !self.is_success()
    }

    /// Log the message
    pub fn log(&self) {
        match self.is_success() {
            true => info!("{}", self.message()),
            false => error!("{}", self.message()),
        }
    }

    /// Convert to anyhow::Error
    pub fn ae(self) -> anyhow::Error {
        self.into_anyhow()
    }

    /// Convert to AppResult
    pub fn ar<T>(self) -> AppResult<T> {
        self.into_result::<T>()
    }

    /// Convert to anyhow::Error
    pub fn into_anyhow(self) -> anyhow::Error {
        anyhow::Error::from(self)
    }

    /// Convert to AppResult
    pub fn into_result<T>(self) -> AppResult<T> {
        Err(anyhow::Error::from(self))
    }
}

impl From<crate::Error> for AppMessage {
    fn from(value: anyhow::Error) -> Self {
        value
            .downcast::<AppMessage>()
            .unwrap_or_else(|e| AppMessage::InternalServerError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_app_message() {
        let message = AppMessage::error_message("Y2k huh?", StatusCode::BAD_REQUEST);
        assert_eq!(message.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(message.message(), "Y2k huh?");

        let message = AppMessage::warning("Invalid pin");
        assert_eq!(message.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(message.message(), "Invalid pin");

        let message = AppMessage::internal_server_error("Y2k ever!");
        assert_eq!(message.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(message.message(), "Y2k ever!");

        let message = AppMessage::unauthorized("Invalid auth token");
        assert_eq!(message.status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(message.message(), "Invalid auth token");

        let message = AppMessage::forbidden("Insufficient permissions");
        assert_eq!(message.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(message.message(), "Insufficient permissions");

        let message = AppMessage::not_found("Could not locate wallet");
        assert_eq!(message.status_code(), StatusCode::NOT_FOUND);
        assert_eq!(message.message(), "Could not locate wallet");

        let message = AppMessage::not_found("Such User does not exist");
        assert_eq!(message.status_code(), StatusCode::NOT_FOUND);
        assert_eq!(message.message(), "Such User does not exist");

        let message =
            AppMessage::missing_environment_variable("DATABASE_URL", env::VarError::NotPresent);
        assert_eq!(message.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            message.message(),
            "Missing environment variable 'DATABASE_URL': environment variable not found"
        );

        #[cfg(feature = "reqwest")]
        {
            let message = AppMessage::reqwest_response_error(ReqwestResponseError::create(
                StatusCode::BAD_REQUEST,
                "Field 'user_id' is required".to_string(),
            ));
            assert_eq!(message.status_code(), StatusCode::BAD_REQUEST);
            assert_eq!(message.message(), "Field 'user_id' is required");
        }

        let message = AppMessage::success("User created");
        assert_eq!(message.status_code(), StatusCode::OK);
        assert_eq!(message.message(), "User created");

        let message = AppMessage::redirect("https://foxtive.com");
        assert_eq!(message.status_code(), StatusCode::FOUND);
        assert_eq!(message.message(), "https://foxtive.com");
    }

    #[test]
    fn test_app_message_is_success() {
        let message = AppMessage::success("User created");
        assert!(message.is_success());
    }

    #[test]
    fn test_app_message_is_error() {
        let message = AppMessage::error_message("Y2k huh?", StatusCode::BAD_REQUEST);
        assert!(message.is_error());

        let message = AppMessage::warning("Invalid pin");
        assert!(message.is_error());

        let message = AppMessage::internal_server_error("Y2k ever!");
        assert!(message.is_error());

        let message = AppMessage::unauthorized("Invalid auth token");
        assert!(message.is_error());

        let message = AppMessage::forbidden("Insufficient permissions");
        assert!(message.is_error());

        let message = AppMessage::not_found("User not found");
        assert!(message.is_error());

        let message =
            AppMessage::missing_environment_variable("DATABASE_URL", env::VarError::NotPresent);
        assert!(message.is_error());
    }

    #[test]
    fn test_app_message_ar() {
        let result = AppMessage::error_message("Y2k huh?", StatusCode::BAD_REQUEST).ar::<()>();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Y2k huh?");
    }

    #[test]
    fn test_app_message_ae() {
        let error = AppMessage::error_message("Y2k huh?", StatusCode::BAD_REQUEST).ae();
        assert_eq!(error.to_string(), "Y2k huh?");
    }
}
