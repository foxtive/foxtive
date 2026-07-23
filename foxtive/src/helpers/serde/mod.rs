//! Serde deserialization helpers for flexible JSON/API field parsing.
//!
//! These functions are designed to be used with `#[serde(deserialize_with = "...")]`
//! to handle APIs that send values in multiple formats (string, number, null, etc.).
//!
//! ## Available Deserializers
//!
//! | Function | Input | Output | Use Case |
//! |----------|-------|--------|----------|
//! | [`deserialize_string_from_any`] | string/number | `String` | IDs that may be numeric |
//! | [`deserialize_optional_string_from_any`] | string/number/null | `Option<String>` | Nullable string fields |
//! | [`deserialize_i64_from_any`] | string/number | `i64` | Numeric IDs as strings |
//! | [`deserialize_optional_i64_from_any`] | string/number/null | `Option<i64>` | Nullable numeric IDs |
//! | [`deserialize_f64_from_any`] | string/number | `f64` | Decimal values as strings |
//! | [`deserialize_optional_f64_from_any`] | string/number/null | `Option<f64>` | Nullable decimals |
//! | [`deserialize_bool_from_any`] | bool/string/number | `bool` | `"true"`, `1`, `true` |
//! | [`deserialize_optional_bool_from_any`] | bool/string/number/null | `Option<bool>` | Nullable booleans |
//! | [`deserialize_optional_timestamp`] | string/number/null | `Option<i64>` | Unix timestamps |
//! | [`deserialize_vec_from_string_or_array`] | array/csv-string | `Vec<String>` | Tag lists |
//! | [`deserialize_optional_vec_from_string_or_array`] | array/csv-string/null | `Option<Vec<String>>` | Nullable tag lists |
//! | [`deserialize_optional_i64_zero_as_none`] | string/number/null | `Option<i64>` | `0` → `None` |
//! | [`deserialize_optional_string_empty_as_none`] | string/null | `Option<String>` | `""` → `None` |
//! | [`deserialize_percentage_to_decimal`] | number/string | `f64` | `50` or `"50%"` → `0.5` |
//! | [`deserialize_i64_with_default`] | string/number/null | `i64` | null → `0` |

pub mod de;

pub use de::*;
