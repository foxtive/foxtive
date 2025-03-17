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

fn get_status_code(status: &AppMessage) -> StatusCode {
    match status {
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
        _ => StatusCode::INTERNAL_SERVER_ERROR, // all database-related errors are 500
    }
}

impl Display for AppMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl AppMessage {
    /// Get the status code
    pub fn status_code(&self) -> StatusCode {
        get_status_code(self)
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
            AppMessage::EntityNotFound(msg) => msg.to_string(),
            AppMessage::ReqwestResponseError(_) => "Internal Server Error".to_string(),
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
        value.downcast::<AppMessage>().unwrap_or_else(|e| {
            AppMessage::ErrorMessage(e.to_string(), StatusCode::INTERNAL_SERVER_ERROR)
        })
    }
}
