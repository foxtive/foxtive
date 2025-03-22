use crate::prelude::AppMessage;
use crate::results::AppResult;

pub trait RecoverAppResultExt<T> {
    fn recover_from<F>(self, func: F) -> AppResult<T>
    where
        F: FnOnce(AppMessage) -> AppResult<T>;
}

pub trait AppErrorExt {
    fn message(&self) -> String;
}
