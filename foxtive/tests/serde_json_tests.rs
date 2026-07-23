//! Tests for serde_json deserialization helpers.

use foxtive::helpers::serde_json::*;
use serde::Deserialize;
use serde_json::json;

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

// Helper structs for testing
#[derive(Deserialize, Debug, PartialEq)]
struct OptionalStringStruct {
    #[serde(deserialize_with = "deserialize_optional_string_from_any")]
    field: Option<String>,
}

#[derive(Deserialize, Debug, PartialEq)]
struct RequiredStringStruct {
    #[serde(deserialize_with = "deserialize_string_from_any")]
    field: String,
}

#[derive(Deserialize, Debug, PartialEq)]
struct I64Struct {
    #[serde(deserialize_with = "deserialize_i64_from_any")]
    field: i64,
}

#[derive(Deserialize, Debug, PartialEq)]
struct F64Struct {
    #[serde(deserialize_with = "deserialize_f64_from_any")]
    field: f64,
}

#[derive(Deserialize)]
struct TestF64 {
    #[serde(default, deserialize_with = "deserialize_optional_f64_from_any")]
    value: Option<f64>,
}

#[derive(Deserialize)]
struct TestI64 {
    #[serde(default, deserialize_with = "deserialize_optional_i64_from_any")]
    id: Option<i64>,
}

#[test]
fn test_deserialize_optional_string_from_any() {
    // String input
    let json = json!({ "field": "hello" });
    let result: OptionalStringStruct = serde_json::from_value(json).unwrap();
    assert_eq!(result.field, Some("hello".to_string()));

    // Number input
    let json = json!({ "field": 123 });
    let result: OptionalStringStruct = serde_json::from_value(json).unwrap();
    assert_eq!(result.field, Some("123".to_string()));

    // Null input
    let json = json!({ "field": null });
    let result: OptionalStringStruct = serde_json::from_value(json).unwrap();
    assert_eq!(result.field, None);

    // Invalid type
    let json = json!({ "field": true });
    let result: Result<OptionalStringStruct, _> = serde_json::from_value(json);
    assert!(result.is_err());
}

#[test]
fn test_deserialize_string_from_any() {
    // String input
    let json = json!({ "field": "hello" });
    let result: RequiredStringStruct = serde_json::from_value(json).unwrap();
    assert_eq!(result.field, "hello".to_string());

    // Number input
    let json = json!({ "field": 42 });
    let result: RequiredStringStruct = serde_json::from_value(json).unwrap();
    assert_eq!(result.field, "42".to_string());

    // Invalid type
    let json = json!({ "field": null });
    let result: Result<RequiredStringStruct, _> = serde_json::from_value(json);
    assert!(result.is_err());
}

#[test]
fn test_deserialize_i64_from_any() {
    // String input
    let json = json!({ "field": "123456" });
    let result: I64Struct = serde_json::from_value(json).unwrap();
    assert_eq!(result.field, 123456);

    // Number input
    let json = json!({ "field": 123456 });
    let result: I64Struct = serde_json::from_value(json).unwrap();
    assert_eq!(result.field, 123456);

    // Negative values
    let json = json!({ "field": -123 });
    let result: I64Struct = serde_json::from_value(json).unwrap();
    assert_eq!(result.field, -123);

    // Invalid string
    let json = json!({ "field": "not_a_number" });
    assert!(serde_json::from_value::<I64Struct>(json).is_err());

    // Float number (should fail for i64)
    let json = json!({ "field": 123.45 });
    assert!(serde_json::from_value::<I64Struct>(json).is_err());

    // Invalid type
    let json = json!({ "field": true });
    assert!(serde_json::from_value::<I64Struct>(json).is_err());
}

#[test]
fn test_deserialize_f64_from_any() {
    // String input
    let json = json!({ "field": "10.5" });
    let result: F64Struct = serde_json::from_value(json).unwrap();
    assert_eq!(result.field, 10.5);

    // Number input
    let json = json!({ "field": 10.5 });
    let result: F64Struct = serde_json::from_value(json).unwrap();
    assert_eq!(result.field, 10.5);

    // Integer input
    let json = json!({ "field": 42 });
    let result: F64Struct = serde_json::from_value(json).unwrap();
    assert_eq!(result.field, 42.0);

    // Invalid string
    let json = json!({ "field": "not_a_float" });
    assert!(serde_json::from_value::<F64Struct>(json).is_err());

    // Invalid type
    let json = json!({ "field": null });
    assert!(serde_json::from_value::<F64Struct>(json).is_err());
}

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
    assert_eq!(
        result.value,
        Some(vec!["a".to_string(), "b".to_string(), "c".to_string()])
    );
}

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
    let invalid_cases = vec![r#"{"value": 101}"#, r#"{"value": -1}"#, r#"{"value": 1.5}"#];

    for json in invalid_cases {
        let result: Result<PercentageTest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Should fail for: {}", json);
    }
}

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

#[test]
fn test_optional_f64() {
    assert_eq!(
        serde_json::from_str::<TestF64>(r#"{"value": 10.5}"#)
            .unwrap()
            .value,
        Some(10.5)
    );

    assert_eq!(
        serde_json::from_str::<TestF64>(r#"{"value": "3.15"}"#)
            .unwrap()
            .value,
        Some(3.15)
    );
    assert_eq!(
        serde_json::from_str::<TestF64>(r#"{"value": null}"#)
            .unwrap()
            .value,
        None
    );
    assert_eq!(
        serde_json::from_str::<TestF64>(r#"{}"#).unwrap().value,
        None
    );
    assert_eq!(
        serde_json::from_str::<TestF64>(r#"{"value": ""}"#)
            .unwrap()
            .value,
        None
    );
    assert!(serde_json::from_str::<TestF64>(r#"{"value": "invalid"}"#).is_err());
}

#[test]
fn test_optional_i64() {
    assert_eq!(
        serde_json::from_str::<TestI64>(r#"{"id": 123}"#)
            .unwrap()
            .id,
        Some(123)
    );
    assert_eq!(
        serde_json::from_str::<TestI64>(r#"{"id": "456"}"#)
            .unwrap()
            .id,
        Some(456)
    );
    assert_eq!(
        serde_json::from_str::<TestI64>(r#"{"id": null}"#)
            .unwrap()
            .id,
        None
    );
    assert_eq!(serde_json::from_str::<TestI64>(r#"{}"#).unwrap().id, None);
    assert_eq!(
        serde_json::from_str::<TestI64>(r#"{"id": ""}"#).unwrap().id,
        None
    );
    assert!(serde_json::from_str::<TestI64>(r#"{"id": "invalid"}"#).is_err());
}
