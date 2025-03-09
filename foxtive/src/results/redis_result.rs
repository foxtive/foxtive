use crate::results::AppResult;
use crate::results::RedisResult;

pub trait RedisResultToAppResult<T> {
    fn into_app_result(self) -> AppResult<T>;
}

pub trait ToLocalRedisResult<T> {
    fn into_redis_result(self) -> RedisResult<T>;
}

impl<T> RedisResultToAppResult<T> for RedisResult<T> {
    fn into_app_result(self) -> AppResult<T> {
        match self {
            Ok(value) => Ok(value),
            Err(err) => Err(err.into()),
        }
    }
}

impl<T> ToLocalRedisResult<T> for redis::RedisResult<T> {
    fn into_redis_result(self) -> RedisResult<T> {
        self
    }
}
