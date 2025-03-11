use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonEmpty {}

pub fn json_empty() -> JsonEmpty {
    JsonEmpty {}
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonResponse<T> {
    pub code: String,
    pub success: bool,
    pub timestamp: u64,
    pub message: Option<String>,
    pub data: T,
}

#[derive(Debug, Serialize)]
pub struct SeJsonResponse<T> {
    pub code: String,
    pub success: bool,
    pub timestamp: u64,
    pub message: Option<String>,
    pub data: T,
}

#[derive(Debug, Deserialize)]
pub struct DeJsonResponse<T> {
    pub code: String,
    pub success: bool,
    pub timestamp: u64,
    pub message: Option<String>,
    pub data: T,
}

impl<T: Serialize> Display for JsonResponse<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(serde_json::to_string(self).unwrap().as_str())
    }
}
