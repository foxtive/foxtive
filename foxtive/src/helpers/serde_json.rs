use serde::{de, Deserialize, Deserializer};
use serde_json::Value;

// ... (your existing functions)

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
        _ => Err(de::Error::custom("Expected boolean, string, number, or null")),
    }
}

/// Deserializes a timestamp that can be a string, number, or null into an `Option<i64>`.
///
/// Useful for API responses where timestamps might be:
/// - Unix timestamp as number: `1234567890`
/// - Unix timestamp as string: `"1234567890"`
/// - ISO 8601 string: `"2023-01-01T00:00:00Z"` (parsed to Unix timestamp)
/// - Null: `null` → `None`
///
/// # Examples
///
/// ```rust
/// use serde::Deserialize;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    // Test structs for each deserializer
    #[derive(Deserialize, Debug, PartialEq)]
    struct BoolTest {
        #[serde(deserialize_with = "deserialize_bool_from_any")]
        value: bool,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct OptionalBoolTest {
        #[serde(deserialize_with = "deserialize_optional_bool_from_any")]
        value: Option<bool>,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct TimestampTest {
        #[serde(deserialize_with = "deserialize_optional_timestamp")]
        value: Option<i64>,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct VecTest {
        #[serde(deserialize_with = "deserialize_vec_from_string_or_array")]
        value: Vec<String>,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct OptionalVecTest {
        #[serde(deserialize_with = "deserialize_optional_vec_from_string_or_array")]
        value: Option<Vec<String>>,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct I64ZeroTest {
        #[serde(deserialize_with = "deserialize_optional_i64_zero_as_none")]
        value: Option<i64>,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct StringEmptyTest {
        #[serde(deserialize_with = "deserialize_optional_string_empty_as_none")]
        value: Option<String>,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct PercentageTest {
        #[serde(deserialize_with = "deserialize_percentage_to_decimal")]
        value: f64,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct I64DefaultTest {
        #[serde(default, deserialize_with = "deserialize_i64_with_default")]
        value: i64,
    }

    // Tests for deserialize_bool_from_any
    #[test]
    fn test_bool_from_boolean() {
        let json = r#"{"value": true}"#;
        let result: BoolTest = serde_json::from_str(json).unwrap();
        assert!(result.value);

        let json = r#"{"value": false}"#;
        let result: BoolTest = serde_json::from_str(json).unwrap();
        assert!(!result.value);
    }

    #[test]
    fn test_bool_from_string() {
        let test_cases = vec![
            (r#"{"value": "true"}"#, true),
            (r#"{"value": "false"}"#, false),
            (r#"{"value": "TRUE"}"#, true),
            (r#"{"value": "FALSE"}"#, false),
            (r#"{"value": "1"}"#, true),
            (r#"{"value": "0"}"#, false),
            (r#"{"value": "yes"}"#, true),
            (r#"{"value": "no"}"#, false),
            (r#"{"value": "on"}"#, true),
            (r#"{"value": "off"}"#, false),
        ];

        for (json, expected) in test_cases {
            let result: BoolTest = serde_json::from_str(json).unwrap();
            assert_eq!(result.value, expected, "Failed for: {}", json);
        }
    }

    #[test]
    fn test_bool_from_number() {
        let json = r#"{"value": 1}"#;
        let result: BoolTest = serde_json::from_str(json).unwrap();
        assert!(result.value);

        let json = r#"{"value": 0}"#;
        let result: BoolTest = serde_json::from_str(json).unwrap();
        assert!(!result.value);
    }

    #[test]
    fn test_bool_invalid() {
        let invalid_cases = vec![
            r#"{"value": "invalid"}"#,
            r#"{"value": 2}"#,
            r#"{"value": -1}"#,
            r#"{"value": []}"#,
            r#"{"value": {}}"#,
        ];

        for json in invalid_cases {
            let result: Result<BoolTest, _> = serde_json::from_str(json);
            assert!(result.is_err(), "Should fail for: {}", json);
        }
    }

    // Tests for deserialize_optional_bool_from_any
    #[test]
    fn test_optional_bool_null() {
        let json = r#"{"value": null}"#;
        let result: OptionalBoolTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, None);
    }

    #[test]
    fn test_optional_bool_values() {
        let json = r#"{"value": "true"}"#;
        let result: OptionalBoolTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, Some(true));

        let json = r#"{"value": 0}"#;
        let result: OptionalBoolTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, Some(false));
    }

    // Tests for deserialize_optional_timestamp
    #[test]
    fn test_timestamp_from_number() {
        let json = r#"{"value": 1234567890}"#;
        let result: TimestampTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, Some(1234567890));
    }

    #[test]
    fn test_timestamp_from_string() {
        let json = r#"{"value": "1234567890"}"#;
        let result: TimestampTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, Some(1234567890));
    }

    #[test]
    fn test_timestamp_null() {
        let json = r#"{"value": null}"#;
        let result: TimestampTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, None);
    }

    #[test]
    fn test_timestamp_invalid() {
        let json = r#"{"value": "not-a-timestamp"}"#;
        let result: Result<TimestampTest, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    // Tests for deserialize_vec_from_string_or_array
    #[test]
    fn test_vec_from_array() {
        let json = r#"{"value": ["item1", "item2", "item3"]}"#;
        let result: VecTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, vec!["item1", "item2", "item3"]);
    }

    #[test]
    fn test_vec_from_array_with_numbers() {
        let json = r#"{"value": ["item1", 123, "item3"]}"#;
        let result: VecTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, vec!["item1", "123", "item3"]);
    }

    #[test]
    fn test_vec_from_comma_separated_string() {
        let json = r#"{"value": "item1,item2,item3"}"#;
        let result: VecTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, vec!["item1", "item2", "item3"]);
    }

    #[test]
    fn test_vec_from_string_with_spaces() {
        let json = r#"{"value": "item1, item2 , item3"}"#;
        let result: VecTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, vec!["item1", "item2", "item3"]);
    }

    #[test]
    fn test_vec_from_single_string() {
        let json = r#"{"value": "single-item"}"#;
        let result: VecTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, vec!["single-item"]);
    }

    // Tests for deserialize_optional_vec_from_string_or_array
    #[test]
    fn test_optional_vec_null() {
        let json = r#"{"value": null}"#;
        let result: OptionalVecTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, None);
    }

    #[test]
    fn test_optional_vec_array() {
        let json = r#"{"value": ["a", "b"]}"#;
        let result: OptionalVecTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, Some(vec!["a".to_string(), "b".to_string()]));
    }

    #[test]
    fn test_optional_vec_string() {
        let json = r#"{"value": "a,b,c"}"#;
        let result: OptionalVecTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, Some(vec!["a".to_string(), "b".to_string(), "c".to_string()]));
    }

    // Tests for deserialize_optional_i64_zero_as_none
    #[test]
    fn test_i64_zero_as_none() {
        let json = r#"{"value": 0}"#;
        let result: I64ZeroTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, None);
    }

    #[test]
    fn test_i64_nonzero() {
        let json = r#"{"value": 42}"#;
        let result: I64ZeroTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, Some(42));

        let json = r#"{"value": -5}"#;
        let result: I64ZeroTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, Some(-5));
    }

    #[test]
    fn test_i64_zero_string() {
        let json = r#"{"value": "0"}"#;
        let result: I64ZeroTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, None);

        let json = r#"{"value": "42"}"#;
        let result: I64ZeroTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, Some(42));
    }

    #[test]
    fn test_i64_null() {
        let json = r#"{"value": null}"#;
        let result: I64ZeroTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, None);
    }

    // Tests for deserialize_optional_string_empty_as_none
    #[test]
    fn test_string_empty_as_none() {
        let json = r#"{"value": ""}"#;
        let result: StringEmptyTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, None);
    }

    #[test]
    fn test_string_nonempty() {
        let json = r#"{"value": "hello"}"#;
        let result: StringEmptyTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, Some("hello".to_string()));
    }

    #[test]
    fn test_string_null() {
        let json = r#"{"value": null}"#;
        let result: StringEmptyTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, None);
    }

    // Tests for deserialize_percentage_to_decimal
    #[test]
    fn test_percentage_as_number() {
        let json = r#"{"value": 50}"#;
        let result: PercentageTest = serde_json::from_str(json).unwrap();
        assert!((result.value - 0.5).abs() < 0.0001);

        let json = r#"{"value": 100}"#;
        let result: PercentageTest = serde_json::from_str(json).unwrap();
        assert!((result.value - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_percentage_as_decimal() {
        let json = r#"{"value": 0.5}"#;
        let result: PercentageTest = serde_json::from_str(json).unwrap();
        assert!((result.value - 0.5).abs() < 0.0001);

        let json = r#"{"value": 0.75}"#;
        let result: PercentageTest = serde_json::from_str(json).unwrap();
        assert!((result.value - 0.75).abs() < 0.0001);
    }

    #[test]
    fn test_percentage_as_string() {
        let json = r#"{"value": "50"}"#;
        let result: PercentageTest = serde_json::from_str(json).unwrap();
        assert!((result.value - 0.5).abs() < 0.0001);

        let json = r#"{"value": "50%"}"#;
        let result: PercentageTest = serde_json::from_str(json).unwrap();
        assert!((result.value - 0.5).abs() < 0.0001);
    }

    #[test]
    fn test_percentage_edge_cases() {
        let json = r#"{"value": 0}"#;
        let result: PercentageTest = serde_json::from_str(json).unwrap();
        assert!((result.value - 0.0).abs() < 0.0001);

        let json = r#"{"value": 1}"#;
        let result: PercentageTest = serde_json::from_str(json).unwrap();
        assert!((result.value - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_percentage_invalid() {
        let invalid_cases = vec![
            r#"{"value": 101}"#,
            r#"{"value": -1}"#,
            r#"{"value": 1.5}"#,
        ];

        for json in invalid_cases {
            let result: Result<PercentageTest, _> = serde_json::from_str(json);
            assert!(result.is_err(), "Should fail for: {}", json);
        }
    }

    // Tests for deserialize_i64_with_default
    #[test]
    fn test_i64_with_default_number() {
        let json = r#"{"value": 42}"#;
        let result: I64DefaultTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, 42);
    }

    #[test]
    fn test_i64_with_default_string() {
        let json = r#"{"value": "123"}"#;
        let result: I64DefaultTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, 123);
    }

    #[test]
    fn test_i64_with_default_null() {
        let json = r#"{"value": null}"#;
        let result: I64DefaultTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, 0);
    }

    #[test]
    fn test_i64_with_default_missing() {
        let json = r#"{}"#;
        let result: I64DefaultTest = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, 0);
    }
}
