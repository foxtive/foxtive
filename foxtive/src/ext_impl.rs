use crate::ext::{AppErrorExt, RecoverAppResultExt};
use crate::prelude::{AppMessage, AppResult};
use crate::Error;

impl<T> RecoverAppResultExt<T> for AppResult<T> {
    fn recover_from<F>(self, func: F) -> AppResult<T>
    where
        F: FnOnce(AppMessage) -> AppResult<T>,
    {
        match self {
            Ok(val) => Ok(val),
            Err(err) => match err.downcast::<AppMessage>() {
                Ok(message) => func(message),
                Err(err) => Err(err),
            },
        }
    }
}

impl<T> RecoverAppResultExt<T> for Error {
    fn recover_from<F>(self, func: F) -> AppResult<T>
    where
        F: FnOnce(AppMessage) -> AppResult<T>,
    {
        match self.downcast::<AppMessage>() {
            Ok(message) => func(message),
            Err(err) => Err(err),
        }
    }
}

impl AppErrorExt for Error {
    fn message(&self) -> String {
        match self.downcast_ref::<AppMessage>() {
            None => self.to_string(),
            Some(msg) => msg.message(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    #[test]
    fn test_recover_from_error() {
        let result = AppMessage::InternalServerError.ae().recover_from(|err| {
            assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(err.message(), "Internal Server Error");
            Ok("recovered".to_string())
        });
        assert_eq!(result.unwrap(), "recovered");
    }

    #[test]
    fn test_recover_from_result() {
        let result = Err(AppMessage::SuccessMessage("User created").ae()).recover_from(|err| {
            assert_eq!(err.status_code(), StatusCode::OK);
            assert_eq!(err.message(), "User created");
            Ok("recovered".to_string())
        });
        assert_eq!(result.unwrap(), "recovered");
    }

    #[test]
    fn test_msg() {
        let result = AppMessage::InternalServerError.ae().message();
        assert_eq!(result, "Internal Server Error");

        let result = AppMessage::WarningMessage("User has already been suspended")
            .ae()
            .message();
        assert_eq!(result, "User has already been suspended");
    }
}
