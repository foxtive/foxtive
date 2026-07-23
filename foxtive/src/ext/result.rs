use crate::enums::AppMessage;
use crate::prelude::AppResult;
use std::borrow::Cow;
use std::future::Future;

pub trait RecoverAppResultExt<T> {
    fn recover_from<F>(self, func: F) -> AppResult<T>
    where
        F: FnOnce(AppMessage) -> AppResult<T>;

    fn recover_from_async<F, Fut>(self, func: F) -> impl Future<Output = AppResult<T>> + Send
    where
        F: FnOnce(AppMessage) -> Fut + Send,
        Fut: Future<Output = AppResult<T>> + Send;
}

pub trait AppErrorExt {
    fn message(&self) -> Cow<'_, str>;
}

impl<T: Send> RecoverAppResultExt<T> for AppResult<T> {
    fn recover_from<F>(self, func: F) -> AppResult<T>
    where
        F: FnOnce(AppMessage) -> AppResult<T>,
    {
        match self {
            Ok(val) => Ok(val),
            Err(err) => func(err),
        }
    }

    async fn recover_from_async<F, Fut>(self, func: F) -> AppResult<T>
    where
        F: FnOnce(AppMessage) -> Fut + Send,
        Fut: Future<Output = AppResult<T>> + Send,
    {
        match self {
            Ok(val) => Ok(val),
            Err(err) => func(err).await,
        }
    }
}

impl AppErrorExt for AppMessage {
    fn message(&self) -> Cow<'_, str> {
        self.message()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{internal_server_error, invalid};
    use http::StatusCode;

    #[test]
    fn test_recover_from_error() {
        let result: AppResult<String> = Err(internal_server_error!("Internal Server Error"))
            .recover_from(|err| {
                assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(err.message(), "Internal Server Error");
                Ok("recovered".to_string())
            });
        assert_eq!(result.unwrap(), "recovered");
    }

    #[test]
    fn test_recover_from_result() {
        let result: AppResult<String> = Err(AppMessage::success("User created"))
            .recover_from(|err| {
                assert_eq!(err.status_code(), StatusCode::OK);
                assert_eq!(err.message(), "User created");
                Ok("recovered".to_string())
            });
        assert_eq!(result.unwrap(), "recovered");
    }

    #[tokio::test]
    async fn test_recover_from_async_error() {
        let result: AppResult<String> = Err(internal_server_error!("Internal Server Error"))
            .recover_from_async(|err| {
                assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(err.message(), "Internal Server Error");
                async { Ok("recovered".to_string()) }
            })
            .await;
        assert_eq!(result.unwrap(), "recovered");
    }

    #[tokio::test]
    async fn test_recover_from_async_result() {
        let result: AppResult<String> = Err(AppMessage::success("User created"))
            .recover_from_async(|err| {
                assert_eq!(err.status_code(), StatusCode::OK);
                assert_eq!(err.message(), "User created");
                async { Ok("recovered".to_string()) }
            })
            .await;
        assert_eq!(result.unwrap(), "recovered");
    }

    #[test]
    fn test_msg() {
        let err = internal_server_error!("Internal Server Error");
        let result = err.message();
        assert_eq!(result, "Internal Server Error");

        let err = invalid!("User has already been suspended");
        let result = err.message();
        assert_eq!(result, "User has already been suspended");
    }
}
