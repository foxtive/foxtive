pub fn to_cent(float: f64) -> i64 {
    (float * 100.00).round() as i64
}

pub fn from_cent(int: i64) -> f64 {
    (int as f64) / 100.00
}

pub fn human_readable(num: f64) -> String {
    let num_str = format!("{:.2}", num); // Ensure two decimal places
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_cent() {
        assert_eq!(to_cent(3.45), 345);
        assert_eq!(to_cent(0.0), 0);
        assert_eq!(to_cent(10.55), 1055);
        assert_eq!(to_cent(-1.23), -123);
    }

    #[test]
    fn test_from_cent() {
        assert_eq!(from_cent(345), 3.45);
        assert_eq!(from_cent(0), 0.0);
        assert_eq!(from_cent(1055), 10.55);
        assert_eq!(from_cent(-123), -1.23);
    }

    #[test]
    fn test_human_readable() {
        assert_eq!(human_readable(1234.56), "1,234.56");
        assert_eq!(human_readable(1000000.0), "1,000,000.00");
        assert_eq!(human_readable(0.0), "0.00");
        assert_eq!(human_readable(-1234.56), "-1,234.56");
    }
}
