use crate::prelude::AppMessage;
use crate::results::AppResult;
use std::env;

pub fn var(env_prefix: &str, key: &str) -> AppResult<String> {
    let key = format!("{env_prefix}_{key}");
    env::var(&key).map_err(|e| AppMessage::MissingEnvironmentVariable(key, e).ae())
}
