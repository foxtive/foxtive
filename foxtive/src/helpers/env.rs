use crate::results::AppResult;
use anyhow::Error;
use std::env;

pub fn var(env_prefix: &str, key: &str) -> AppResult<String> {
    env::var(&format!("{}_{}", env_prefix, key)).map_err(Error::msg)
}
