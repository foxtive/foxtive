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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_empty_creates_empty_struct() {
        let empty = json_empty();
        let serialized = serde_json::to_string(&empty).unwrap();
        assert_eq!(serialized, "{}");
    }

    #[test]
    fn json_response_display_formats_as_json() {
        let response = JsonResponse {
            code: "000".to_string(),
            success: true,
            timestamp: 1234567890,
            message: Some("OK".to_string()),
            data: "hello",
        };
        let display = format!("{response}");
        assert!(display.contains("\"code\":\"000\""));
        assert!(display.contains("\"success\":true"));
        assert!(display.contains("\"data\":\"hello\""));
    }

    #[test]
    fn json_response_round_trip_serialization() {
        let response = JsonResponse {
            code: "200".to_string(),
            success: true,
            timestamp: 999,
            message: None,
            data: 42,
        };
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: JsonResponse<i32> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.code, "200");
        assert_eq!(deserialized.data, 42);
        assert!(deserialized.message.is_none());
    }

    #[test]
    fn json_response_with_null_message() {
        let response = JsonResponse {
            code: "000".to_string(),
            success: false,
            timestamp: 0,
            message: None,
            data: (),
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["message"], serde_json::Value::Null);
    }
}
