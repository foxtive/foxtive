pub mod app_result;
#[cfg(feature = "redis")]
pub mod redis_result;

pub type AppResult<T> = anyhow::Result<T>;

pub type AppOptionalResult<T> = AppResult<Option<T>>;

#[cfg(feature = "redis")]
pub type RedisResult<T> = Result<T, redis::RedisError>;

#[cfg(feature = "database")]
pub type AppPaginationResult<T> = AppResult<crate::database::pagination::PageData<T>>;
