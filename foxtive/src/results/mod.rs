//! Result types and error conversions.
//!
//! The canonical result type is [`AppResult<T>`](type.AppResult.html) which is
//! `Result<T, AppMessage>`. Infrastructure error conversions from common
//! error types (IO, serde, Diesel, etc.) are provided in [`infra_conversions`].

pub mod app_result;
pub mod infra_conversions;
#[cfg(feature = "redis")]
pub mod redis_result;

use crate::enums::AppMessage;

pub type AppResult<T> = Result<T, AppMessage>;

pub type AppOptionalResult<T> = AppResult<Option<T>>;

#[cfg(feature = "redis")]
pub type RedisResult<T> = Result<T, redis::RedisError>;

#[cfg(any(feature = "database", feature = "database-async"))]
pub type AppPaginationResult<T> = AppResult<crate::database::pagination::PageData<T>>;
