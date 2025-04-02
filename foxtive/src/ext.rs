use std::future::Future;
use crate::prelude::AppMessage;
use crate::results::AppResult;

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
    fn message(&self) -> String;
}