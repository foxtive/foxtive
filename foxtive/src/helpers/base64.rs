//! Base64 encoding and decoding functionality.
//!
//! This module provides a simple interface for encoding strings to Base64 format
//! and decoding Base64 strings back to their original form. It uses the standard
//! Base64 alphabet as defined in RFC 4648.
//!
//! # Examples
//!
//! ```
//! use foxtive::helpers::base64::Base64;;
//!
//! // Encoding
//! let encoded = Base64::encode("hello world").unwrap();
//! assert_eq!(encoded, "aGVsbG8gd29ybGQ=");
//!
//! // Decoding
//! let decoded = Base64::decode("aGVsbG8gd29ybGQ=").unwrap();
//! assert_eq!(decoded, "hello world");
//! ```
//!
//! # Error Handling
//!
//! Both encoding and decoding operations return `AppResult<String>`. The following
//! scenarios may result in errors:
//!
//! - Decoding invalid Base64 strings
//! - Decoding Base64 strings that result in invalid UTF-8

use base64::{engine, Engine};
use crate::prelude::AppResult;

/// A utility struct providing Base64 encoding and decoding functionality.
#[derive(Debug)]
pub struct Base64;

impl Base64 {
    /// Encodes a string slice into Base64 format.
    ///
    /// # Arguments
    ///
    /// * `str` - The input string slice to encode
    ///
    /// # Returns
    ///
    /// Returns `AppResult<String>` containing the Base64 encoded string.
    ///
    /// # Examples
    ///
    /// ```
    /// use foxtive::helpers::base64::Base64;;
    ///
    /// let encoded = Base64::encode("hello world").unwrap();
    /// assert_eq!(encoded, "aGVsbG8gd29ybGQ=");
    /// ```
    pub fn encode(str: &str) -> AppResult<String> {
        Ok(engine::general_purpose::STANDARD.encode(str))
    }

    /// Decodes a Base64 encoded string back to its original form.
    ///
    /// # Arguments
    ///
    /// * `str` - The Base64 encoded string to decode
    ///
    /// # Returns
    ///
    /// Returns `AppResult<String>` containing the decoded string.
    ///
    /// # Errors
    ///
    /// Will return an error if:
    /// - The input is not valid Base64
    /// - The decoded bytes do not form valid UTF-8
    ///
    /// # Examples
    ///
    /// ```
    /// use foxtive::helpers::base64::Base64;;
    ///
    /// let decoded = Base64::decode("aGVsbG8gd29ybGQ=").unwrap();
    /// assert_eq!(decoded, "hello world");
    ///
    /// // Invalid Base64 string will result in an error
    /// assert!(Base64::decode("invalid_base64").is_err());
    /// ```
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
