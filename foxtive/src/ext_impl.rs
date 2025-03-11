use crate::Error;
use crate::ext::RecoverAppResultExt;
use crate::prelude::{AppMessage, AppResult};

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
