use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonEmpty {}

pub fn json_empty() -> JsonEmpty {
    JsonEmpty {}
}
