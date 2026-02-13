/// Alias for `format_currency` - formats a number with commas and two decimal places.
pub use format_currency as format_number;

/// Alias for `format_integer` - formats an integer with commas.
pub use format_integer as format_number_int;

/// Converts a dollar amount to cents.
///
/// # Examples
///
/// ```
/// use foxtive::helpers::number::to_cents;
///
/// assert_eq!(to_cents(3.45), 345);
/// assert_eq!(to_cents(10.55), 1055);
/// assert_eq!(to_cents(-1.23), -123);
/// ```
pub fn to_cents(amount: f64) -> i64 {
    (amount * 100.00).round() as i64
}

/// Converts cents to a dollar amount.
///
/// # Examples
///
/// ```
/// use foxtive::helpers::number::from_cents;
///
/// assert_eq!(from_cents(345), 3.45);
/// assert_eq!(from_cents(1055), 10.55);
/// assert_eq!(from_cents(-123), -1.23);
/// ```
pub fn from_cents(cents: i64) -> f64 {
    (cents as f64) / 100.00
}

/// Formats any numeric type with thousand separators and exactly two decimal places.
///
/// Works with floats (`f32`, `f64`) and integers (`i32`, `i64`, `u32`, `u64`, etc.).
/// Integers are displayed with `.00` at the end.
///
/// # Examples
///
/// ```
/// use foxtive::helpers::number::format_currency;
///
/// assert_eq!(format_currency(1234.56), "1,234.56");
/// assert_eq!(format_currency(1000000.0), "1,000,000.00");
/// assert_eq!(format_currency(-1234.56), "-1,234.56");
/// assert_eq!(format_currency(1234), "1,234.00");
/// assert_eq!(format_currency(5000_i32), "5,000.00");
/// ```
pub fn format_currency<T>(num: T) -> String
where
    T: Into<f64> + Copy,
{
    let num_float: f64 = num.into();
    let num_str = format!("{num_float:.2}"); // Ensure two decimal places
    let parts: Vec<&str> = num_str.split('.').collect(); // Split into integer and fractional parts

    let mut integer_part = parts[0].to_string();
    let fractional_part = parts[1]; // There will always be a fractional part now

    let mut str_num = String::new();
    let mut negative = false;
    let values: Vec<char> = integer_part.chars().collect();

    if values[0] == '-' {
        integer_part.remove(0);
        negative = true;
    }

    for (i, char) in integer_part.chars().rev().enumerate() {
        if i % 3 == 0 && i != 0 {
            str_num.insert(0, ',');
        }
        str_num.insert(0, char);
    }

    if negative {
        str_num.insert(0, '-');
    }

    str_num.push('.');
    str_num.push_str(fractional_part);

    str_num
}

/// Formats any integer type with thousand separators.
///
/// Works with `i32`, `i64`, `u32`, `u64`, and other types that implement `Display`.
///
/// # Examples
///
/// ```
/// use foxtive::helpers::number::format_integer;
///
/// assert_eq!(format_integer(1234), "1,234");
/// assert_eq!(format_integer(1000000_i64), "1,000,000");
/// assert_eq!(format_integer(-1234), "-1,234");
/// ```
pub fn format_integer<T: std::fmt::Display>(num: T) -> String {
    let num_str = num.to_string();
    let mut result = String::new();
    let mut negative = false;

    let mut chars: Vec<char> = num_str.chars().collect();

    // Handle negative numbers
    if chars.first() == Some(&'-') {
        chars.remove(0);
        negative = true;
    }

    // Add commas from right to left
    for (i, &ch) in chars.iter().rev().enumerate() {
        if i % 3 == 0 && i != 0 {
            result.insert(0, ',');
        }
        result.insert(0, ch);
    }

    if negative {
        result.insert(0, '-');
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_integer() {
        assert_eq!(format_integer(1234), "1,234");
        assert_eq!(format_integer(1000000), "1,000,000");
        assert_eq!(format_integer(0), "0");
        assert_eq!(format_integer(-1234), "-1,234");
        assert_eq!(format_integer(999_i64), "999");
        assert_eq!(format_integer(1000_u32), "1,000");
    }

    #[test]
    fn test_to_cents() {
        assert_eq!(to_cents(3.45), 345);
        assert_eq!(to_cents(0.0), 0);
        assert_eq!(to_cents(10.55), 1055);
        assert_eq!(to_cents(-1.23), -123);
    }

    #[test]
    fn test_from_cents() {
        assert_eq!(from_cents(345), 3.45);
        assert_eq!(from_cents(0), 0.0);
        assert_eq!(from_cents(1055), 10.55);
        assert_eq!(from_cents(-123), -1.23);
    }

    #[test]
    fn test_format_currency() {
        // Float tests
        assert_eq!(format_currency(1234.56), "1,234.56");
        assert_eq!(format_currency(1000000.0), "1,000,000.00");
        assert_eq!(format_currency(0.0), "0.00");
        assert_eq!(format_currency(-1234.56), "-1,234.56");

        // Integer tests - formatted as currency
        assert_eq!(format_currency(1234), "1,234.00");
        assert_eq!(format_currency(5000_i32), "5,000.00");
        assert_eq!(format_currency(-999), "-999.00");
    }
}
