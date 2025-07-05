use uuid::Uuid;

pub struct Str;

impl Str {
    pub fn uc_first(s: &str) -> String {
        let mut chars = s.chars();
        match chars.next() {
            None => String::new(),
            Some(first_char) => first_char.to_uppercase().collect::<String>() + chars.as_str(),
        }
    }

    pub fn uc_words(s: &str) -> String {
        s.split_whitespace()
            .map(Self::uc_first)
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[cfg(feature = "regex")]
    pub fn is_username_valid(name: String) -> Box<fancy_regex::Result<bool>> {
        crate::helpers::Regex::validate_username(&name)
    }

    /// Generate uuid v4 based id with dashes(-) removed
    pub fn uuid() -> String {
        Uuid::new_v4().to_string().replace("-", "")
    }

    /// Truncates a string to a specified length, adding ellipsis if truncated
    pub fn truncate(s: &str, max_length: usize) -> String {
        if s.len() <= max_length {
            s.to_string()
        } else {
            format!("{}...", &s[..max_length])
        }
    }

    /// Removes all whitespace characters from a string
    pub fn remove_whitespace(s: &str) -> String {
        s.chars().filter(|c| !c.is_whitespace()).collect()
    }

    /// Reverses a string
    pub fn reverse(s: &str) -> String {
        s.chars().rev().collect()
    }

    /// Counts occurrences of a substring in a string
    pub fn count_occurrences(s: &str, substr: &str) -> usize {
        if substr.is_empty() {
            return 0;
        }
        s.matches(substr).count()
    }

    /// Checks if a string contains only digits
    pub fn is_numeric(s: &str) -> bool {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
    }

    /// Checks if a string contains only alphabetic characters
    pub fn is_alphabetic(s: &str) -> bool {
        !s.is_empty() && s.chars().all(|c| c.is_alphabetic())
    }

    /// Converts snake_case to camelCase
    pub fn camel_case(s: &str) -> String {
        let mut result = String::new();
        let mut capitalize_next = false;

        for (i, c) in s.chars().enumerate() {
            if c == '_' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(c.to_ascii_uppercase());
                capitalize_next = false;
            } else if i == 0 {
                result.push(c.to_ascii_lowercase());
            } else {
                result.push(c);
            }
        }
        result
    }

    /// Pads the string to the left with a specified character until it reaches the given length
    pub fn pad_left(s: &str, width: usize, pad_char: char) -> String {
        if s.len() >= width {
            s.to_string()
        } else {
            format!("{}{}", pad_char.to_string().repeat(width - s.len()), s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uc_first() {
        assert_eq!(Str::uc_first("hello"), "Hello");
        assert_eq!(Str::uc_first("rust"), "Rust");
        assert_eq!(Str::uc_first(""), ""); // Test empty string
        assert_eq!(Str::uc_first("a"), "A"); // Test single character
        assert_eq!(Str::uc_first("hELLO"), "HELLO"); // Test capitalizing first char but not modifying others
        assert_eq!(Str::uc_first("1world"), "1world"); // Test first character is non-alphabetic
    }

    #[test]
    fn test_uc_words() {
        assert_eq!(Str::uc_words("hello world"), "Hello World");
        assert_eq!(
            Str::uc_words("rust programming language"),
            "Rust Programming Language"
        );
        assert_eq!(Str::uc_words(""), ""); // Test empty string
        assert_eq!(Str::uc_words("a b c"), "A B C"); // Test single characters
        assert_eq!(Str::uc_words("multiple    spaces"), "Multiple Spaces"); // Test multiple spaces
        assert_eq!(Str::uc_words("123 hello"), "123 Hello"); // Test with non-alphabetic characters
    }

    #[cfg(feature = "regex")]
    #[test]
    fn test_is_username_valid_valid_usernames() {
        assert!(Str::is_username_valid("a".to_string()).unwrap());
        assert!(Str::is_username_valid("abc1234".to_string()).unwrap());
        assert!(Str::is_username_valid("a.b.c".to_string()).unwrap());
        assert!(Str::is_username_valid("username1".to_string()).unwrap());
        assert!(Str::is_username_valid("a123456789012345678901234567890123".to_string()).unwrap());
        // 37 chars
    }

    #[cfg(feature = "regex")]
    #[test]
    fn test_is_username_valid_invalid_usernames() {
        assert!(!Str::is_username_valid("1username".to_string()).unwrap()); // Starts with a digit
        assert!(!Str::is_username_valid("username!".to_string()).unwrap()); // Invalid character
        assert!(!Str::is_username_valid("".to_string()).unwrap()); // Empty username
        assert!(
            !Str::is_username_valid(
                "a.b.c.d.e.f.g.h.i.j.k.l.m.n.o.p.q.r.s.t.u.v.w.x.y.z".to_string()
            )
            .unwrap()
        ); // More than 37 chars
    }

    #[test]
    fn test_uuid() {
        let uuid = Str::uuid();
        // Check if the length is 32 (UUID v4 without dashes)
        assert_eq!(uuid.len(), 32);
        // Check if it contains only hexadecimal characters
        assert!(uuid.chars().all(|c| c.is_ascii_hexdigit()));

        // Generate a few UUIDs and check that they are unique
        let uuid_set: std::collections::HashSet<_> = (0..1000).map(|_| Str::uuid()).collect();
        assert_eq!(uuid_set.len(), 1000); // Check for uniqueness
    }

    #[test]
    fn test_truncate() {
        assert_eq!(Str::truncate("Hello, World!", 5), "Hello...");
        assert_eq!(Str::truncate("Hello", 10), "Hello");
        assert_eq!(Str::truncate("", 5), "");
    }

    #[test]
    fn test_remove_whitespace() {
        assert_eq!(Str::remove_whitespace("Hello World"), "HelloWorld");
        assert_eq!(Str::remove_whitespace("   spaces   "), "spaces");
        assert_eq!(Str::remove_whitespace("\t\ntest\r"), "test");
    }

    #[test]
    fn test_reverse() {
        assert_eq!(Str::reverse("hello"), "olleh");
        assert_eq!(Str::reverse(""), "");
        assert_eq!(Str::reverse("Rust"), "tsuR");
    }

    #[test]
    fn test_count_occurrences() {
        assert_eq!(Str::count_occurrences("hello hello hello", "hello"), 3);
        assert_eq!(Str::count_occurrences("aaa", "aa"), 1);
        assert_eq!(Str::count_occurrences("test", ""), 0);
    }

    #[test]
    fn test_is_numeric() {
        assert!(Str::is_numeric("123"));
        assert!(!Str::is_numeric("12.3"));
        assert!(!Str::is_numeric("abc"));
        assert!(!Str::is_numeric(""));
    }

    #[test]
    fn test_is_alphabetic() {
        assert!(Str::is_alphabetic("abc"));
        assert!(Str::is_alphabetic("ABC"));
        assert!(!Str::is_alphabetic("abc123"));
        assert!(!Str::is_alphabetic(""));
    }

    #[test]
    fn test_to_camel_case() {
        assert_eq!(Str::camel_case("hello_world"), "helloWorld");
        assert_eq!(Str::camel_case("user_id"), "userId");
        assert_eq!(Str::camel_case("already_camelCase"), "alreadyCamelCase");
        assert_eq!(Str::camel_case(""), "");
    }

    #[test]
    fn test_pad_left() {
        assert_eq!(Str::pad_left("123", 5, '0'), "00123");
        assert_eq!(Str::pad_left("abc", 3, '0'), "abc");
        assert_eq!(Str::pad_left("", 2, '*'), "**");
    }
}

#[cfg(test)]
mod ext_tests {
    // use super::{StringExt, Str};

    use crate::ext::StringExt;

    #[test]
    fn test_uc_first_ext() {
        assert_eq!("hello".uc_first(), "Hello");
        assert_eq!("hELLO".uc_first(), "HELLO");
        assert_eq!("".uc_first(), "");
        assert_eq!("1world".uc_first(), "1world");
        assert_eq!(String::from("hello").uc_first(), "Hello");
    }

    #[test]
    fn test_uc_words_ext() {
        assert_eq!("hello world".uc_words(), "Hello World");
        assert_eq!(
            "rust programming language".uc_words(),
            "Rust Programming Language"
        );
        assert_eq!("".uc_words(), "");
        assert_eq!("a b c".uc_words(), "A B C");
        assert_eq!(
            String::from("multiple    spaces").uc_words(),
            "Multiple Spaces"
        );
    }

    #[cfg(feature = "regex")]
    #[test]
    fn test_is_username_valid_ext() {
        assert!("a".is_username_valid().unwrap());
        assert!(String::from("abc1234").is_username_valid().unwrap());
        assert!(!"".is_username_valid().unwrap());
    }

    #[test]
    fn test_truncate_ext() {
        assert_eq!("Hello, World!".truncate(5), "Hello...");
        assert_eq!("Hello".truncate(10), "Hello");
        assert_eq!("".truncate(5), "");
        assert_eq!(String::from("Hello, World!").truncate(5), "Hello...");
    }

    #[test]
    fn test_remove_whitespace_ext() {
        assert_eq!("Hello World".remove_whitespace(), "HelloWorld");
        assert_eq!("   spaces   ".remove_whitespace(), "spaces");
        assert_eq!("\t\ntest\r".remove_whitespace(), "test");
        assert_eq!(String::from(" a b c ").remove_whitespace(), "abc");
    }

    #[test]
    fn test_reverse_ext() {
        assert_eq!("hello".reverse(), "olleh");
        assert_eq!("".reverse(), "");
        assert_eq!(String::from("Rust").reverse(), "tsuR");
    }

    #[test]
    fn test_count_occurrences_ext() {
        assert_eq!("hello hello hello".count_occurrences("hello"), 3);
        assert_eq!("aaa".count_occurrences("aa"), 1);
        assert_eq!("test".count_occurrences(""), 0);
        assert_eq!(String::from("aabbcc").count_occurrences("b"), 2);
    }

    #[test]
    fn test_is_numeric_ext() {
        assert!("123".is_numeric());
        assert!(!"12.3".is_numeric());
        assert!(!"abc".is_numeric());
        assert!(!"".is_numeric());
        assert!(String::from("456").is_numeric());
    }

    #[test]
    fn test_is_alphabetic_ext() {
        assert!("abc".is_alphabetic());
        assert!("ABC".is_alphabetic());
        assert!(!"abc123".is_alphabetic());
        assert!(!"".is_alphabetic());
        assert!(String::from("xyzXYZ").is_alphabetic());
    }

    #[test]
    fn test_camel_case_ext() {
        assert_eq!("hello_world".camel_case(), "helloWorld");
        assert_eq!("user_id".camel_case(), "userId");
        assert_eq!("already_camelCase".camel_case(), "alreadyCamelCase");
        assert_eq!("".camel_case(), "");
        assert_eq!(String::from("foo_bar_baz").camel_case(), "fooBarBaz");
    }

    #[test]
    fn test_pad_left_ext() {
        assert_eq!("123".pad_left(5, '0'), "00123");
        assert_eq!("abc".pad_left(3, '0'), "abc");
        assert_eq!("".pad_left(2, '*'), "**");
        assert_eq!(String::from("42").pad_left(4, '-'), "--42");
    }
}
