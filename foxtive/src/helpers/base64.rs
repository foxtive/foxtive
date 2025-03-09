use base64::{engine, Engine};

use crate::prelude::AppResult;

pub struct Base64;

impl Base64 {
    pub fn encode(str: &str) -> AppResult<String> {
        Ok(engine::general_purpose::STANDARD.encode(str))
    }

    pub fn decode(str: &str) -> AppResult<String> {
        Ok(String::from_utf8(
            engine::general_purpose::STANDARD.decode(str)?,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        let input = "hello world";
        let encoded = Base64::encode(input).unwrap();
        assert_eq!(encoded, "aGVsbG8gd29ybGQ=");
    }

    #[test]
    fn test_base64_decode() {
        let input = "aGVsbG8gd29ybGQ=";
        let decoded = Base64::decode(input).unwrap();
        assert_eq!(decoded, "hello world");
    }

    #[test]
    fn test_base64_encode_empty_string() {
        let input = "";
        let encoded = Base64::encode(input).unwrap();
        assert_eq!(encoded, "");
    }

    #[test]
    fn test_base64_decode_empty_string() {
        let input = "";
        let decoded = Base64::decode(input).unwrap();
        assert_eq!(decoded, "");
    }

    #[test]
    fn test_base64_decode_invalid_string() {
        let input = "invalid_base64";
        let result = Base64::decode(input);
        assert!(result.is_err());
    }
}
