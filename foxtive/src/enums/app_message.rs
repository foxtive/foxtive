#[cfg(feature = "reqwest")]
use crate::helpers::reqwest::ReqwestResponseError;
use crate::results::AppResult;
use http::StatusCode;
use std::fmt::{Debug, Display, Formatter};
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum AppMessage {
    Unauthorized,
    Forbidden,
    InternalServerError,
    ErrorMessage(String, StatusCode),
    InternalServerErrorMessage(&'static str),
    Redirect(&'static str),
    SuccessMessage(&'static str),
    SuccessMessageString(String),
    WarningMessage(&'static str),
    WarningMessageString(String),
    UnAuthorizedMessage(&'static str),
    UnAuthorizedMessageString(String),
    ForbiddenMessage(&'static str),
    ForbiddenMessageString(String),
    EntityNotFound(String),
    #[cfg(feature = "reqwest")]
    ReqwestResponseError(ReqwestResponseError),
}

impl Display for AppMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl AppMessage {
    /// Get the status code
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppMessage::SuccessMessage(_) | AppMessage::SuccessMessageString(_) => StatusCode::OK,
            AppMessage::WarningMessage(_) | AppMessage::WarningMessageString(_) => {
                StatusCode::BAD_REQUEST
            }
            AppMessage::ErrorMessage(_, status) => *status,
            AppMessage::Unauthorized
            | AppMessage::UnAuthorizedMessage(_)
            | AppMessage::UnAuthorizedMessageString(_) => StatusCode::UNAUTHORIZED,
            AppMessage::Forbidden
            | AppMessage::ForbiddenMessage(_)
            | AppMessage::ForbiddenMessageString(_) => StatusCode::FORBIDDEN,
            AppMessage::EntityNotFound(_) => StatusCode::NOT_FOUND,
            #[cfg(feature = "reqwest")]
            AppMessage::ReqwestResponseError(err) => *err.code(),
            AppMessage::Redirect(_) => StatusCode::FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR, // all database-related errors are 500
        }
    }

    /// Get the message
    pub fn message(&self) -> String {
        match self {
            AppMessage::Unauthorized => "Unauthorized".to_string(),
            AppMessage::Forbidden => "Forbidden".to_string(),
            AppMessage::InternalServerError => "Internal Server Error".to_string(),
            AppMessage::ErrorMessage(msg, _) => msg.to_owned(),
            AppMessage::InternalServerErrorMessage(msg) => msg.to_string(),
            AppMessage::Redirect(msg) => msg.to_string(),
            AppMessage::SuccessMessage(msg) => msg.to_string(),
            AppMessage::SuccessMessageString(msg) => msg.to_string(),
            AppMessage::WarningMessage(msg) => msg.to_string(),
            AppMessage::WarningMessageString(msg) => msg.to_string(),
            AppMessage::UnAuthorizedMessage(msg) => msg.to_string(),
            AppMessage::UnAuthorizedMessageString(msg) => msg.to_string(),
            AppMessage::ForbiddenMessage(msg) => msg.to_string(),
            AppMessage::ForbiddenMessageString(msg) => msg.to_string(),
            AppMessage::EntityNotFound(entity) => format!("Such {} does not exist", entity),
            #[cfg(feature = "reqwest")]
            AppMessage::ReqwestResponseError(_) => "Internal Server Error".to_string(),
        }
    }

    /// Check if the message is a success
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            AppMessage::SuccessMessage(_) | AppMessage::SuccessMessageString(_)
        )
    }

    /// Check if the message is an error
    pub fn is_error(&self) -> bool {
        !self.is_success()
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
        value.downcast::<AppMessage>().unwrap_or_else(|e| {
            AppMessage::ErrorMessage(e.to_string(), StatusCode::INTERNAL_SERVER_ERROR)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_message() {
        let message = AppMessage::ErrorMessage("Y2k huh?".to_string(), StatusCode::BAD_REQUEST);
        assert_eq!(message.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(message.message(), "Y2k huh?");

        let message = AppMessage::WarningMessage("Invalid pin");
        assert_eq!(message.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(message.message(), "Invalid pin");

        let message = AppMessage::InternalServerErrorMessage("Y2k ever!");
        assert_eq!(message.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(message.message(), "Y2k ever!");

        let message = AppMessage::UnAuthorizedMessage("Invalid auth token");
        assert_eq!(message.status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(message.message(), "Invalid auth token");

        let message = AppMessage::ForbiddenMessage("Insufficient permissions");
        assert_eq!(message.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(message.message(), "Insufficient permissions");

        let message = AppMessage::EntityNotFound("User".to_string());
        assert_eq!(message.status_code(), StatusCode::NOT_FOUND);
        assert_eq!(message.message(), "Such User does not exist");

        #[cfg(feature = "reqwest")]
        {
            let message = AppMessage::ReqwestResponseError(ReqwestResponseError::create(
                StatusCode::BAD_REQUEST,
                "Field 'user_id' is required".to_string(),
            ));
            assert_eq!(message.status_code(), StatusCode::BAD_REQUEST);
            assert_eq!(message.message(), "Internal Server Error");
        }

        let message = AppMessage::SuccessMessage("User created");
        assert_eq!(message.status_code(), StatusCode::OK);
        assert_eq!(message.message(), "User created");

        let message = AppMessage::SuccessMessageString("User created".to_string());
        assert_eq!(message.status_code(), StatusCode::OK);
        assert_eq!(message.message(), "User created");

        let message = AppMessage::Redirect("https://foxtive.com");
        assert_eq!(message.status_code(), StatusCode::FOUND);
        assert_eq!(message.message(), "https://foxtive.com");
    }

    #[test]
    fn test_app_message_is_success() {
        let message = AppMessage::SuccessMessage("User created");
        assert!(message.is_success());
    }

    #[test]
    fn test_app_message_is_error() {
        let message = AppMessage::ErrorMessage("Y2k huh?".to_string(), StatusCode::BAD_REQUEST);
        assert!(message.is_error());

        let message = AppMessage::WarningMessage("Invalid pin");
        assert!(message.is_error());

        let message = AppMessage::InternalServerErrorMessage("Y2k ever!");
        assert!(message.is_error());

        let message = AppMessage::UnAuthorizedMessage("Invalid auth token");
        assert!(message.is_error());

        let message = AppMessage::ForbiddenMessage("Insufficient permissions");
        assert!(message.is_error());

        let message = AppMessage::EntityNotFound("User".to_string());
        assert!(message.is_error());
    }

    #[test]
    fn test_app_message_ar() {
        let result = AppMessage::ErrorMessage("Y2k huh?".to_string(), StatusCode::BAD_REQUEST).ar::<()>();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Y2k huh?");
    }

    #[test]
    fn test_app_message_ae() {
        let error = AppMessage::ErrorMessage("Y2k huh?".to_string(), StatusCode::BAD_REQUEST).ae();
        assert_eq!(error.to_string(), "Y2k huh?");
    }
}
