use crate::helpers::json::{json_empty, JsonEmpty};
use crate::results::AppResult;
#[cfg(feature = "database")]
use diesel::result::Error;
#[cfg(feature = "database")]
use diesel::QueryResult;
use serde::Serialize;

pub trait IntoAppResult<T> {
    fn into_app_result(self) -> AppResult<T>;
}

pub trait IntoEmptyJson {
    fn into_empty_json(self) -> AppResult<JsonEmpty>;
}

impl<T: Serialize> IntoEmptyJson for AppResult<T> {
    fn into_empty_json(self) -> AppResult<JsonEmpty> {
        Ok(json_empty())
    }
}

#[cfg(feature = "database")]
impl<T> IntoAppResult<T> for QueryResult<T> {
    fn into_app_result(self) -> AppResult<T> {
        match self {
            Ok(value) => Ok(value),
            Err(Error::NotFound) => {
                Err(AppMessage::EntityNotFound("".to_string()).into())
            }
            Err(e) => Err(e.into()),
        }
    }
}
