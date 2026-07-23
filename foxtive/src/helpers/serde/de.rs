//! Deserialization helpers for flexible JSON/API field parsing.
//!
//! Each function handles a specific type coercion pattern commonly needed when
//! working with APIs that send values in inconsistent formats.

use serde::{Deserialize, Deserializer, de};
use serde_json::Value;

/// Deserializes an optional field that can be either a string or a number into an `Option<String>`.
///
/// This is useful for API responses where a field might be:
/// - A string: `"123"` or `"abc"`
/// - A number: `123`
/// - Null or missing: `null`
///
/// # Examples
///
/// ```rust
/// use serde::Deserialize;
/// use foxtive::helpers::serde_json::deserialize_optional_string_from_any;
///
/// #[derive(Deserialize)]
/// struct Response {
///     #[serde(deserialize_with = "deserialize_optional_string_from_any")]
///     id: Option<String>,
/// }
/// ```
pub fn deserialize_optional_string_from_any<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    let value: Option<Value> = Option::deserialize(deserializer)?;
    Ok(match value {
        Some(Value::String(s)) => Some(s),
        Some(Value::Number(num)) => Some(num.to_string()),
        None => None,
        _ => return Err(de::Error::custom("Expected string, number, or null")),
    })
}

/// Deserializes a required field that can be either a string or a number into a `String`.
///
/// This is useful for API responses where a field is always present but the type varies:
/// - A string: `"123"` or `"abc"`
/// - A number: `123`
///
/// # Errors
///
/// Returns an error if the value is not a string or number.
///
/// # Examples
///
/// ```rust
/// use serde::Deserialize;
/// use foxtive::helpers::serde_json::deserialize_string_from_any;
///
/// #[derive(Deserialize)]
/// struct Response {
///     #[serde(deserialize_with = "deserialize_string_from_any")]
///     id: String,
/// }
/// ```
pub fn deserialize_string_from_any<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    let value: Value = Value::deserialize(deserializer)?;
    match value {
        Value::String(s) => Ok(s),
        Value::Number(num) => Ok(num.to_string()),
        _ => Err(de::Error::custom("Expected string or number")),
    }
}

/// Deserializes a field that can be either a string or a number into an `i64`.
///
/// This is useful for API responses where numeric IDs might be represented as:
/// - A string: `"123456"`
/// - A number: `123456`
///
/// # Errors
///
/// Returns an error if:
/// - The value is not a string or number
/// - The string cannot be parsed as an i64
/// - The number is not a valid i64
///
/// # Examples
///
/// ```rust
/// use serde::Deserialize;
/// use foxtive::helpers::serde_json::deserialize_i64_from_any;
///
/// #[derive(Deserialize)]
/// struct Response {
///     #[serde(deserialize_with = "deserialize_i64_from_any")]
///     id: i64,
/// }
/// ```
pub fn deserialize_i64_from_any<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<i64, D::Error> {
    let value: Value = Value::deserialize(deserializer)?;
    match value {
        Value::String(s) => s.parse::<i64>().map_err(de::Error::custom),
        Value::Number(num) => num
            .as_i64()
            .ok_or_else(|| de::Error::custom("Invalid number")),
        _ => Err(de::Error::custom("Expected string or number")),
    }
}

/// Deserializes a field that can be either a string, number, or null into an `Option<i64>`.
///
/// This is useful for API responses where numeric IDs might be represented as:
/// - A string: `"123456"`
/// - A number: `123456`
/// - Null or absent: `null` or field not present
///
/// # Errors
///
/// Returns an error if:
/// - The value is not a string, number, or null
/// - The string cannot be parsed as an i64
/// - The number is not a valid i64
///
/// # Examples
///
/// ```rust
/// use serde::Deserialize;
/// use foxtive::helpers::serde_json::deserialize_optional_i64_from_any;
///
/// #[derive(Deserialize)]
/// struct Response {
///     #[serde(default, deserialize_with = "deserialize_optional_i64_from_any")]
///     id: Option<i64>,
/// }
/// ```
pub fn deserialize_optional_i64_from_any<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;

    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse::<i64>().map(Some).map_err(de::Error::custom)
            }
        }
        Some(Value::Number(num)) => num
            .as_i64()
            .ok_or_else(|| de::Error::custom("Invalid number"))
            .map(Some),
        _ => Err(de::Error::custom("Expected string, number, or null")),
    }
}

/// Deserializes a field that can be either a string or a number into an `f64`.
///
/// This is useful for API responses where floating-point values might be represented as:
/// - A string: `"10.5"` or `"3.14159"`
/// - A number: `10.5` or `3.14159`
///
/// # Errors
///
/// Returns an error if:
/// - The value is not a string or number
/// - The string cannot be parsed as an f64
/// - The number is not a valid f64
///
/// # Examples
///
/// ```rust
/// use serde::Deserialize;
/// use foxtive::helpers::serde_json::deserialize_f64_from_any;
///
/// #[derive(Deserialize)]
/// struct Measurement {
///     #[serde(deserialize_with = "deserialize_f64_from_any")]
///     value: f64,
/// }
/// ```
pub fn deserialize_f64_from_any<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;

    match value {
        Value::Number(num) => num
            .as_f64()
            .ok_or_else(|| de::Error::custom("Invalid number")),
        Value::String(s) => s.parse::<f64>().map_err(de::Error::custom),
        _ => Err(de::Error::custom("Expected a number or string")),
    }
}

/// Deserializes a field that can be either a string, number, or null into an `Option<f64>`.
///
/// This is useful for API responses where floating-point values might be represented as:
/// - A string: `"10.5"` or `"3.14159"`
/// - A number: `10.5` or `3.14159`
/// - Null or absent: `null` or field not present
///
/// # Errors
///
/// Returns an error if:
/// - The value is not a string, number, or null
/// - The string cannot be parsed as an f64
/// - The number is not a valid f64
///
/// # Examples
///
/// ```rust
/// use serde::Deserialize;
/// use foxtive::helpers::serde_json::deserialize_optional_f64_from_any;
///
/// #[derive(Deserialize)]
/// struct Measurement {
///     #[serde(default, deserialize_with = "deserialize_optional_f64_from_any")]
///     value: Option<f64>,
/// }
/// ```
pub fn deserialize_optional_f64_from_any<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;

    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(num)) => num
            .as_f64()
            .ok_or_else(|| de::Error::custom("Invalid number"))
            .map(Some),
        Some(Value::String(s)) => {
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse::<f64>().map(Some).map_err(de::Error::custom)
            }
        }
        _ => Err(de::Error::custom("Expected a number, string, or null")),
    }
}

/// Deserializes a boolean field that can be represented as a string, number, or boolean.
///
/// Handles multiple representations of boolean values:
/// - Boolean: `true`, `false`
/// - String: `"true"`, `"false"`, `"1"`, `"0"`, `"yes"`, `"no"` (case-insensitive)
/// - Number: `1` (true), `0` (false)
///
/// # Errors
///
/// Returns an error if the value cannot be interpreted as a boolean.
///
/// # Examples
///
/// ```rust
/// use serde::Deserialize;
/// use foxtive::helpers::serde_json::deserialize_bool_from_any;
///
/// #[derive(Deserialize)]
/// struct Settings {
///     #[serde(deserialize_with = "deserialize_bool_from_any")]
///     enabled: bool,
/// }
/// ```
pub fn deserialize_bool_from_any<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<bool, D::Error> {
    let value: Value = Value::deserialize(deserializer)?;
    match value {
        Value::Bool(b) => Ok(b),
        Value::String(s) => match s.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(true),
            "false" | "0" | "no" | "off" => Ok(false),
            _ => Err(de::Error::custom(format!("Invalid boolean string: {}", s))),
        },
        Value::Number(num) => match num.as_i64() {
            Some(1) => Ok(true),
            Some(0) => Ok(false),
            _ => Err(de::Error::custom("Boolean number must be 0 or 1")),
        },
        _ => Err(de::Error::custom("Expected boolean, string, or number")),
    }
}

/// Deserializes an optional boolean field that can be represented as a string, number, or boolean.
///
/// Same as `deserialize_bool_from_any` but returns `None` for null values.
///
/// # Examples
///
/// ```rust
/// use serde::Deserialize;
/// use foxtive::helpers::serde_json::deserialize_optional_bool_from_any;
///
/// #[derive(Deserialize)]
/// struct Settings {
///     #[serde(deserialize_with = "deserialize_optional_bool_from_any")]
///     enabled: Option<bool>,
/// }
/// ```
pub fn deserialize_optional_bool_from_any<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<bool>, D::Error> {
    let value: Option<Value> = Option::deserialize(deserializer)?;
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(b)),
        Some(Value::String(s)) => match s.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(Some(true)),
            "false" | "0" | "no" | "off" => Ok(Some(false)),
            _ => Err(de::Error::custom(format!("Invalid boolean string: {}", s))),
        },
        Some(Value::Number(num)) => match num.as_i64() {
            Some(1) => Ok(Some(true)),
            Some(0) => Ok(Some(false)),
            _ => Err(de::Error::custom("Boolean number must be 0 or 1")),
        },
        _ => Err(de::Error::custom(
            "Expected boolean, string, number, or null",
        )),
    }
}

/// Deserializes a timestamp that can be a string, number, or null into an `Option<i64>`.
///
/// Useful for API responses where timestamps might be:
/// - Unix timestamp as numbers: `1234567890`
/// - Unix timestamp as string: `"1234567890"`
/// - Null: `null` → `None`
///
/// # Examples
///
/// ```rust
/// use serde::Deserialize;
/// use foxtive::helpers::serde_json::deserialize_optional_timestamp;
///
/// #[derive(Deserialize)]
/// struct Event {
///     #[serde(deserialize_with = "deserialize_optional_timestamp")]
///     created_at: Option<i64>,
/// }
/// ```
pub fn deserialize_optional_timestamp<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<i64>, D::Error> {
    let value: Option<Value> = Option::deserialize(deserializer)?;
    Ok(match value {
        Some(Value::String(s)) => {
            // Try parsing as Unix timestamp first
            if let Ok(timestamp) = s.parse::<i64>() {
                Some(timestamp)
            } else {
                // Try parsing as ISO 8601 or other date format
                // You might want to use chrono or time crate for this
                return Err(de::Error::custom("ISO 8601 parsing not implemented"));
            }
        }
        Some(Value::Number(num)) => Some(
            num.as_i64()
                .ok_or_else(|| de::Error::custom("Invalid timestamp"))?,
        ),
        None | Some(Value::Null) => None,
        _ => return Err(de::Error::custom("Expected string, number, or null")),
    })
}

/// Deserializes a comma-separated string or array into a `Vec<String>`.
///
/// Handles multiple input formats:
/// - Array: `["item1", "item2"]`
/// - Comma-separated string: `"item1,item2,item3"`
/// - Single string: `"item1"`
///
/// # Examples
///
/// ```rust
/// use serde::Deserialize;
/// use foxtive::helpers::serde_json::deserialize_vec_from_string_or_array;
///
/// #[derive(Deserialize)]
/// struct Product {
///     #[serde(deserialize_with = "deserialize_vec_from_string_or_array")]
///     tags: Vec<String>,
/// }
/// ```
pub fn deserialize_vec_from_string_or_array<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    let value: Value = Value::deserialize(deserializer)?;
    match value {
        Value::Array(arr) => arr
            .into_iter()
            .map(|v| match v {
                Value::String(s) => Ok(s),
                Value::Number(n) => Ok(n.to_string()),
                _ => Err(de::Error::custom("Array items must be strings or numbers")),
            })
            .collect(),
        Value::String(s) => Ok(s.split(',').map(|s| s.trim().to_string()).collect()),
        _ => Err(de::Error::custom("Expected array or string")),
    }
}

/// Deserializes an optional comma-separated string or array into an `Option<Vec<String>>`.
///
/// Same as `deserialize_vec_from_string_or_array` but returns `None` for null values.
///
/// # Examples
///
/// ```rust
/// use serde::Deserialize;
/// use foxtive::helpers::serde_json::deserialize_optional_vec_from_string_or_array;
///
/// #[derive(Deserialize)]
/// struct Product {
///     #[serde(deserialize_with = "deserialize_optional_vec_from_string_or_array")]
///     tags: Option<Vec<String>>,
/// }
/// ```
pub fn deserialize_optional_vec_from_string_or_array<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error> {
    let value: Option<Value> = Option::deserialize(deserializer)?;
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(arr)) => {
            let result: Result<Vec<String>, _> = arr
                .into_iter()
                .map(|v| match v {
                    Value::String(s) => Ok(s),
                    Value::Number(n) => Ok(n.to_string()),
                    _ => Err(de::Error::custom("Array items must be strings or numbers")),
                })
                .collect();
            result.map(Some)
        }
        Some(Value::String(s)) => Ok(Some(s.split(',').map(|s| s.trim().to_string()).collect())),
        _ => Err(de::Error::custom("Expected array, string, or null")),
    }
}

/// Deserializes an optional numeric field, treating zero as `None`.
///
/// Useful for APIs where `0` is used to represent "no value" or "not set".
/// - Any non-zero number: `Some(value)`
/// - Zero: `None`
/// - Null: `None`
/// - String representation: parsed accordingly
///
/// # Examples
///
/// ```rust
/// use serde::Deserialize;
/// use foxtive::helpers::serde_json::deserialize_optional_i64_zero_as_none;
///
/// #[derive(Deserialize)]
/// struct Account {
///     #[serde(deserialize_with = "deserialize_optional_i64_zero_as_none")]
///     parent_id: Option<i64>, // 0 becomes None
/// }
/// ```
pub fn deserialize_optional_i64_zero_as_none<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<i64>, D::Error> {
    let value: Option<Value> = Option::deserialize(deserializer)?;
    match value {
        Some(Value::String(s)) => {
            let num = s.parse::<i64>().map_err(de::Error::custom)?;
            Ok(if num == 0 { None } else { Some(num) })
        }
        Some(Value::Number(num)) => {
            let val = num
                .as_i64()
                .ok_or_else(|| de::Error::custom("Invalid number"))?;
            Ok(if val == 0 { None } else { Some(val) })
        }
        None | Some(Value::Null) => Ok(None),
        _ => Err(de::Error::custom("Expected string, number, or null")),
    }
}

/// Deserializes an empty string as `None` and non-empty strings as `Some(String)`.
///
/// Useful for APIs where empty strings represent null values.
/// - Non-empty string: `Some("value")`
/// - Empty string: `None`
/// - Null: `None`
///
/// # Examples
///
/// ```rust
/// use serde::Deserialize;
/// use foxtive::helpers::serde_json::deserialize_optional_string_empty_as_none;
///
/// #[derive(Deserialize)]
/// struct User {
///     #[serde(deserialize_with = "deserialize_optional_string_empty_as_none")]
///     middle_name: Option<String>, // "" becomes None
/// }
/// ```
pub fn deserialize_optional_string_empty_as_none<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    let value: Option<Value> = Option::deserialize(deserializer)?;
    match value {
        Some(Value::String(s)) if !s.is_empty() => Ok(Some(s)),
        Some(Value::String(_)) => Ok(None), // empty string
        None | Some(Value::Null) => Ok(None),
        _ => Err(de::Error::custom("Expected string or null")),
    }
}

/// Deserializes a percentage value (0-100 or 0.0-1.0) into a normalized float (0.0-1.0).
///
/// Handles multiple representations:
/// - Percentage as number: `50` → `0.5`, `0.5` → `0.5`
/// - Percentage as string: `"50"` → `0.5`, `"50%"` → `0.5`
/// - Automatically detects if value is already normalized (0.0-1.0) or percentage (0-100)
///
/// # Examples
///
/// ```rust
/// use serde::Deserialize;
/// use foxtive::helpers::serde_json::deserialize_percentage_to_decimal;
///
/// #[derive(Deserialize)]
/// struct Discount {
///     #[serde(deserialize_with = "deserialize_percentage_to_decimal")]
///     rate: f64, // stored as 0.0-1.0
/// }
/// ```
pub fn deserialize_percentage_to_decimal<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<f64, D::Error> {
    let value: Value = Value::deserialize(deserializer)?;
    let num = match value {
        Value::Number(n) => n
            .as_f64()
            .ok_or_else(|| de::Error::custom("Invalid number"))?,
        Value::String(s) => {
            let cleaned = s.trim().trim_end_matches('%');
            cleaned.parse::<f64>().map_err(de::Error::custom)?
        }
        _ => return Err(de::Error::custom("Expected number or string")),
    };

    // Reject fractional values above 1.0 (e.g., 1.5)
    if num > 1.0 && num <= 100.0 {
        if num.fract() != 0.0 {
            return Err(de::Error::custom(
                "Percentage values above 1.0 must be whole numbers (e.g., 25, 50, 100)",
            ));
        }
        Ok(num / 100.0)
    } else if (0.0..=1.0).contains(&num) {
        Ok(num)
    } else {
        Err(de::Error::custom(
            "Percentage must be between 0-100 or 0.0-1.0",
        ))
    }
}

/// Deserializes a number with default value if null or missing.
///
/// # Examples
///
/// ```rust
/// use serde::Deserialize;
/// use foxtive::helpers::serde_json::deserialize_i64_with_default;
///
/// #[derive(Deserialize)]
/// struct Config {
///     #[serde(default, deserialize_with = "deserialize_i64_with_default")]
///     retry_count: i64, // defaults to 0 if null
/// }
/// ```
pub fn deserialize_i64_with_default<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<i64, D::Error> {
    let value: Option<Value> = Option::deserialize(deserializer)?;
    match value {
        Some(Value::String(s)) => s.parse::<i64>().map_err(de::Error::custom),
        Some(Value::Number(num)) => num
            .as_i64()
            .ok_or_else(|| de::Error::custom("Invalid number")),
        None | Some(Value::Null) => Ok(0),
        _ => Err(de::Error::custom("Expected string, number, or null")),
    }
}
