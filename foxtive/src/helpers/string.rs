use uuid::Uuid;

pub struct StringHelper;

impl StringHelper {
    pub fn uc_first(s: &str) -> String {
        let mut chars = s.chars();
        match chars.next() {
            None => String::new(),
            Some(first_char) => {
                let rest: String = chars.as_str().to_lowercase();
                first_char.to_uppercase().to_string() + &rest
            }
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
        crate::helpers::regex::Tester::validate_username(&name)
    }

    /// Generate uuid v4 based id with dashes(-) removed
    pub fn uuid() -> String {
        Uuid::new_v4().as_simple().to_string()
    }

    /// Truncates a string to a specified length, adding ellipsis if truncated.
    ///
    /// Respects UTF-8 character boundaries - will not split a multi-byte character.
    pub fn truncate(s: &str, max_length: usize) -> String {
        if s.len() <= max_length {
            s.to_string()
        } else {
            let mut end = max_length;
            while end > 0 && !s.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}...", &s[..end])
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

    /// Converts a string into a URL-friendly slug.
    ///
    /// Lowercases the string, replaces non-alphanumeric characters with hyphens,
    /// collapses consecutive hyphens, and trims leading/trailing hyphens.
    pub fn slugify(s: &str) -> String {
        let mut slug = String::with_capacity(s.len());
        let mut prev_hyphen = false;

        for c in s.chars() {
            if c.is_alphanumeric() {
                slug.push(c.to_ascii_lowercase());
                prev_hyphen = false;
            } else if !prev_hyphen && !slug.is_empty() {
                slug.push('-');
                prev_hyphen = true;
            }
        }

        slug.trim_end_matches('-').to_string()
    }

    /// Converts a string to snake_case.
    ///
    /// Handles camelCase, PascalCase, kebab-case, and space-separated words.
    pub fn snake_case(s: &str) -> String {
        if s.is_empty() {
            return String::new();
        }

        let mut result = String::with_capacity(s.len() + 4);
        let chars: Vec<char> = s.chars().collect();

        for (i, &c) in chars.iter().enumerate() {
            if c == '-' || c == ' ' || c == '_' {
                if !result.is_empty() && !result.ends_with('_') {
                    result.push('_');
                }
            } else if c.is_ascii_uppercase() {
                let prev_is_lower = i > 0 && chars[i - 1].is_ascii_lowercase();
                let next_is_lower = i + 1 < chars.len() && chars[i + 1].is_ascii_lowercase();

                if !result.is_empty() && !result.ends_with('_') && prev_is_lower || next_is_lower {
                    result.push('_');
                }
                result.push(c.to_ascii_lowercase());
            } else {
                result.push(c);
            }
        }

        result.trim_end_matches('_').to_string()
    }

    /// Converts a string to kebab-case.
    ///
    /// Handles camelCase, PascalCase, snake_case, and space-separated words.
    pub fn kebab_case(s: &str) -> String {
        Self::snake_case(s).replace('_', "-")
    }

    /// Converts a string to PascalCase.
    ///
    /// Handles snake_case, kebab-case, camelCase, and space-separated words.
    pub fn pascal_case(s: &str) -> String {
        if s.is_empty() {
            return String::new();
        }

        let mut result = String::with_capacity(s.len());
        let mut capitalize_next = true;

        for c in s.chars() {
            if c == '_' || c == '-' || c == ' ' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(c.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(c);
            }
        }

        result
    }

    /// Converts a string to Title Case (alias for uc_words).
    pub fn title_case(s: &str) -> String {
        Self::uc_words(s)
    }

    /// Checks if the string is a valid email format (basic check).
    pub fn is_email(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }

        let parts: Vec<&str> = s.split('@').collect();
        if parts.len() != 2 {
            return false;
        }

        let local = parts[0];
        let domain = parts[1];

        if local.is_empty() || domain.is_empty() {
            return false;
        }

        if !domain.contains('.') {
            return false;
        }

        let domain_parts: Vec<&str> = domain.split('.').collect();
        if domain_parts.iter().any(|p| p.is_empty()) {
            return false;
        }

        true
    }

    /// Checks if the string is a valid URL format (basic check).
    pub fn is_url(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }

        let lower = s.to_lowercase();
        if !lower.starts_with("http://") && !lower.starts_with("https://") {
            return false;
        }

        let without_scheme = if lower.starts_with("https://") {
            &s[8..]
        } else {
            &s[7..]
        };

        !without_scheme.is_empty() && without_scheme.contains('.')
    }

    /// Checks if the string is a valid UUID format.
    pub fn is_uuid(s: &str) -> bool {
        if s.len() != 36 {
            return false;
        }

        let chars: Vec<char> = s.chars().collect();
        for (i, c) in chars.iter().enumerate() {
            if [8, 13, 18, 23].contains(&i) {
                if *c != '-' {
                    return false;
                }
            } else if !c.is_ascii_hexdigit() {
                return false;
            }
        }

        true
    }

    /// Splits the string into words.
    pub fn words(s: &str) -> Vec<&str> {
        s.split_whitespace().collect()
    }

    /// Splits the string into lines.
    pub fn lines(s: &str) -> Vec<&str> {
        s.lines().collect()
    }

    /// Repeats the string n times.
    pub fn repeat(s: &str, n: usize) -> String {
        s.repeat(n)
    }

    /// Checks if the string starts with any of the given prefixes.
    pub fn starts_with_any(s: &str, prefixes: &[&str]) -> bool {
        prefixes.iter().any(|p| s.starts_with(p))
    }

    /// Checks if the string ends with any of the given suffixes.
    pub fn ends_with_any(s: &str, suffixes: &[&str]) -> bool {
        suffixes.iter().any(|p| s.ends_with(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uc_first() {
        assert_eq!(StringHelper::uc_first("hello"), "Hello");
        assert_eq!(StringHelper::uc_first("rust"), "Rust");
        assert_eq!(StringHelper::uc_first(""), "");
        assert_eq!(StringHelper::uc_first("a"), "A");
        assert_eq!(StringHelper::uc_first("hELLO"), "Hello");
        assert_eq!(StringHelper::uc_first("1world"), "1world");
    }

    #[test]
    fn test_uc_words() {
        assert_eq!(StringHelper::uc_words("PLANET MARS"), "Planet Mars");
        assert_eq!(StringHelper::uc_words("hello world"), "Hello World");
        assert_eq!(
            StringHelper::uc_words("rust programming language"),
            "Rust Programming Language"
        );
        assert_eq!(StringHelper::uc_words(""), ""); // Test empty string
        assert_eq!(StringHelper::uc_words("a b c"), "A B C"); // Test single characters
        assert_eq!(
            StringHelper::uc_words("multiple    spaces"),
            "Multiple Spaces"
        ); // Test multiple spaces
        assert_eq!(StringHelper::uc_words("123 hello"), "123 Hello"); // Test with non-alphabetic characters
    }

    #[cfg(feature = "regex")]
    #[test]
    fn test_is_username_valid_valid_usernames() {
        assert!(StringHelper::is_username_valid("a".to_string()).unwrap());
        assert!(StringHelper::is_username_valid("abc1234".to_string()).unwrap());
        assert!(StringHelper::is_username_valid("a.b.c".to_string()).unwrap());
        assert!(StringHelper::is_username_valid("username1".to_string()).unwrap());
        assert!(
            StringHelper::is_username_valid("a123456789012345678901234567890123".to_string())
                .unwrap()
        );
        // 37 chars
    }

    #[cfg(feature = "regex")]
    #[test]
    fn test_is_username_valid_invalid_usernames() {
        assert!(!StringHelper::is_username_valid("1username".to_string()).unwrap()); // Starts with a digit
        assert!(!StringHelper::is_username_valid("username!".to_string()).unwrap()); // Invalid character
        assert!(!StringHelper::is_username_valid("".to_string()).unwrap()); // Empty username
        assert!(
            !StringHelper::is_username_valid(
                "a.b.c.d.e.f.g.h.i.j.k.l.m.n.o.p.q.r.s.t.u.v.w.x.y.z".to_string()
            )
            .unwrap()
        ); // More than 37 chars
    }

    #[test]
    fn test_uuid() {
        let uuid = StringHelper::uuid();
        // Check if the length is 32 (UUID v4 without dashes)
        assert_eq!(uuid.len(), 32);
        // Check if it contains only hexadecimal characters
        assert!(uuid.chars().all(|c| c.is_ascii_hexdigit()));

        // Generate a few UUIDs and check that they are unique
        let uuid_set: std::collections::HashSet<_> =
            (0..1000).map(|_| StringHelper::uuid()).collect();
        assert_eq!(uuid_set.len(), 1000); // Check for uniqueness
    }

    #[test]
    fn test_truncate() {
        assert_eq!(StringHelper::truncate("Hello, World!", 5), "Hello...");
        assert_eq!(StringHelper::truncate("Hello", 10), "Hello");
        assert_eq!(StringHelper::truncate("", 5), "");
    }

    #[test]
    fn test_remove_whitespace() {
        assert_eq!(StringHelper::remove_whitespace("Hello World"), "HelloWorld");
        assert_eq!(StringHelper::remove_whitespace("   spaces   "), "spaces");
        assert_eq!(StringHelper::remove_whitespace("\t\ntest\r"), "test");
    }

    #[test]
    fn test_reverse() {
        assert_eq!(StringHelper::reverse("hello"), "olleh");
        assert_eq!(StringHelper::reverse(""), "");
        assert_eq!(StringHelper::reverse("Rust"), "tsuR");
    }

    #[test]
    fn test_count_occurrences() {
        assert_eq!(
            StringHelper::count_occurrences("hello hello hello", "hello"),
            3
        );
        assert_eq!(StringHelper::count_occurrences("aaa", "aa"), 1);
        assert_eq!(StringHelper::count_occurrences("test", ""), 0);
    }

    #[test]
    fn test_is_numeric() {
        assert!(StringHelper::is_numeric("123"));
        assert!(!StringHelper::is_numeric("12.3"));
        assert!(!StringHelper::is_numeric("abc"));
        assert!(!StringHelper::is_numeric(""));
    }

    #[test]
    fn test_is_alphabetic() {
        assert!(StringHelper::is_alphabetic("abc"));
        assert!(StringHelper::is_alphabetic("ABC"));
        assert!(!StringHelper::is_alphabetic("abc123"));
        assert!(!StringHelper::is_alphabetic(""));
    }

    #[test]
    fn test_to_camel_case() {
        assert_eq!(StringHelper::camel_case("hello_world"), "helloWorld");
        assert_eq!(StringHelper::camel_case("user_id"), "userId");
        assert_eq!(
            StringHelper::camel_case("already_camelCase"),
            "alreadyCamelCase"
        );
        assert_eq!(StringHelper::camel_case(""), "");
    }

    #[test]
    fn test_pad_left() {
        assert_eq!(StringHelper::pad_left("123", 5, '0'), "00123");
        assert_eq!(StringHelper::pad_left("abc", 3, '0'), "abc");
        assert_eq!(StringHelper::pad_left("", 2, '*'), "**");
    }

    #[test]
    fn test_slugify() {
        assert_eq!(StringHelper::slugify("Hello MARS"), "hello-mars");
        assert_eq!(StringHelper::slugify("Hello World"), "hello-world");
        assert_eq!(StringHelper::slugify("  Foo   Bar  "), "foo-bar");
        assert_eq!(StringHelper::slugify("Rust is great!"), "rust-is-great");
        assert_eq!(StringHelper::slugify("already-slug"), "already-slug");
        assert_eq!(
            StringHelper::slugify("---leading-trailing---"),
            "leading-trailing"
        );
        assert_eq!(StringHelper::slugify(""), "");
        assert_eq!(StringHelper::slugify("CamelCase Text"), "camelcase-text");
    }

    #[test]
    fn test_snake_case() {
        assert_eq!(StringHelper::snake_case("camelCase"), "camel_case");
        assert_eq!(StringHelper::snake_case("PascalCase"), "pascal_case");
        assert_eq!(StringHelper::snake_case("kebab-case"), "kebab_case");
        assert_eq!(StringHelper::snake_case("hello world"), "hello_world");
        assert_eq!(StringHelper::snake_case("already_snake"), "already_snake");
        assert_eq!(StringHelper::snake_case("HTMLParser"), "html_parser");
        assert_eq!(StringHelper::snake_case(""), "");
        assert_eq!(StringHelper::snake_case("ABC"), "abc");
    }

    #[test]
    fn test_kebab_case() {
        assert_eq!(StringHelper::kebab_case("camelCase"), "camel-case");
        assert_eq!(StringHelper::kebab_case("PascalCase"), "pascal-case");
        assert_eq!(StringHelper::kebab_case("snake_case"), "snake-case");
        assert_eq!(StringHelper::kebab_case("hello world"), "hello-world");
        assert_eq!(StringHelper::kebab_case("already-kebab"), "already-kebab");
        assert_eq!(StringHelper::kebab_case(""), "");
    }

    #[test]
    fn test_pascal_case() {
        assert_eq!(StringHelper::pascal_case("snake_case"), "SnakeCase");
        assert_eq!(StringHelper::pascal_case("kebab-case"), "KebabCase");
        assert_eq!(StringHelper::pascal_case("camelCase"), "CamelCase");
        assert_eq!(StringHelper::pascal_case("hello world"), "HelloWorld");
        assert_eq!(StringHelper::pascal_case("AlreadyPascal"), "AlreadyPascal");
        assert_eq!(StringHelper::pascal_case(""), "");
    }

    #[test]
    fn test_title_case() {
        assert_eq!(StringHelper::title_case("hello world"), "Hello World");
        assert_eq!(
            StringHelper::title_case("rust programming"),
            "Rust Programming"
        );
        assert_eq!(StringHelper::title_case(""), "");
    }

    #[test]
    fn test_is_email() {
        assert!(StringHelper::is_email("user@example.com"));
        assert!(StringHelper::is_email("test.user@domain.org"));
        assert!(StringHelper::is_email("a@b.c"));
        assert!(!StringHelper::is_email(""));
        assert!(!StringHelper::is_email("invalid"));
        assert!(!StringHelper::is_email("@domain.com"));
        assert!(!StringHelper::is_email("user@"));
        assert!(!StringHelper::is_email("user@domain"));
        assert!(!StringHelper::is_email("user@.com"));
        assert!(!StringHelper::is_email("user@domain."));
    }

    #[test]
    fn test_is_url() {
        assert!(StringHelper::is_url("http://example.com"));
        assert!(StringHelper::is_url("https://example.com"));
        assert!(StringHelper::is_url("https://sub.domain.org/path"));
        assert!(!StringHelper::is_url(""));
        assert!(!StringHelper::is_url("example.com"));
        assert!(!StringHelper::is_url("ftp://example.com"));
        assert!(!StringHelper::is_url("http://"));
        assert!(!StringHelper::is_url("https://nodot"));
    }

    #[test]
    fn test_is_uuid() {
        assert!(StringHelper::is_uuid(
            "550e8400-e29b-41d4-a716-446655440000"
        ));
        assert!(StringHelper::is_uuid(
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8"
        ));
        assert!(!StringHelper::is_uuid(""));
        assert!(!StringHelper::is_uuid(
            "550e8400-e29b-41d4-a716-44665544000"
        ));
        assert!(!StringHelper::is_uuid("550e8400e29b41d4a716446655440000"));
        assert!(!StringHelper::is_uuid(
            "550e8400-e29b-41d4-a716-44665544000g"
        ));
    }

    #[test]
    fn test_words() {
        assert_eq!(StringHelper::words("hello world"), vec!["hello", "world"]);
        assert_eq!(
            StringHelper::words("  multiple   spaces  "),
            vec!["multiple", "spaces"]
        );
        assert_eq!(StringHelper::words(""), Vec::<&str>::new());
        assert_eq!(StringHelper::words("single"), vec!["single"]);
    }

    #[test]
    fn test_lines() {
        assert_eq!(
            StringHelper::lines("line1\nline2\nline3"),
            vec!["line1", "line2", "line3"]
        );
        assert_eq!(StringHelper::lines("single"), vec!["single"]);
        assert_eq!(StringHelper::lines(""), Vec::<&str>::new());
    }

    #[test]
    fn test_repeat() {
        assert_eq!(StringHelper::repeat("abc", 3), "abcabcabc");
        assert_eq!(StringHelper::repeat("x", 5), "xxxxx");
        assert_eq!(StringHelper::repeat("hello", 0), "");
        assert_eq!(StringHelper::repeat("", 10), "");
    }

    #[test]
    fn test_starts_with_any() {
        assert!(StringHelper::starts_with_any(
            "hello world",
            &["hello", "hi"]
        ));
        assert!(StringHelper::starts_with_any("hi there", &["hello", "hi"]));
        assert!(!StringHelper::starts_with_any("hey", &["hello", "hi"]));
        assert!(!StringHelper::starts_with_any("test", &[]));
        assert!(!StringHelper::starts_with_any("", &["a", "b"]));
    }

    #[test]
    fn test_ends_with_any() {
        assert!(StringHelper::ends_with_any(
            "hello world",
            &["world", "earth"]
        ));
        assert!(StringHelper::ends_with_any(
            "hello earth",
            &["world", "earth"]
        ));
        assert!(!StringHelper::ends_with_any("hello", &["world", "earth"]));
        assert!(!StringHelper::ends_with_any("test", &[]));
        assert!(!StringHelper::ends_with_any("", &["a", "b"]));
    }
}

#[cfg(test)]
mod ext_tests {
    // use super::{StringExt, Str};

    use crate::ext::StringExt;

    #[test]
    fn test_uc_first_ext() {
        assert_eq!("hello".uc_first(), "Hello");
        assert_eq!("hELLO".uc_first(), "Hello");
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

    #[test]
    fn test_snake_case_ext() {
        assert_eq!("camelCase".snake_case(), "camel_case");
        assert_eq!("PascalCase".snake_case(), "pascal_case");
        assert_eq!(String::from("hello world").snake_case(), "hello_world");
    }

    #[test]
    fn test_kebab_case_ext() {
        assert_eq!("camelCase".kebab_case(), "camel-case");
        assert_eq!("snake_case".kebab_case(), "snake-case");
        assert_eq!(String::from("hello world").kebab_case(), "hello-world");
    }

    #[test]
    fn test_pascal_case_ext() {
        assert_eq!("snake_case".pascal_case(), "SnakeCase");
        assert_eq!("hello world".pascal_case(), "HelloWorld");
        assert_eq!(String::from("kebab-case").pascal_case(), "KebabCase");
    }

    #[test]
    fn test_title_case_ext() {
        assert_eq!("hello world".title_case(), "Hello World");
        assert_eq!(
            String::from("rust programming").title_case(),
            "Rust Programming"
        );
    }

    #[test]
    fn test_is_email_ext() {
        assert!("user@example.com".is_email());
        assert!(!"invalid".is_email());
        assert!(String::from("a@b.c").is_email());
    }

    #[test]
    fn test_is_url_ext() {
        assert!("https://example.com".is_url());
        assert!(!"example.com".is_url());
        assert!(String::from("http://test.org").is_url());
    }

    #[test]
    fn test_is_uuid_ext() {
        assert!("550e8400-e29b-41d4-a716-446655440000".is_uuid());
        assert!(!"not-a-uuid".is_uuid());
        assert!(String::from("6ba7b810-9dad-11d1-80b4-00c04fd430c8").is_uuid());
    }

    #[test]
    fn test_words_ext() {
        assert_eq!("hello world".words(), vec!["hello", "world"]);
        assert_eq!(String::from("single").words(), vec!["single"]);
    }

    #[test]
    fn test_lines_ext() {
        assert_eq!(StringExt::lines("a\nb\nc"), vec!["a", "b", "c"]);
        assert_eq!(String::from("single").lines(), vec!["single"]);
    }

    #[test]
    fn test_repeat_str_ext() {
        assert_eq!("abc".repeat_str(3), "abcabcabc");
        assert_eq!(String::from("x").repeat_str(4), "xxxx");
    }

    #[test]
    fn test_starts_with_any_ext() {
        assert!("hello".starts_with_any(&["he", "hi"]));
        assert!(!"hello".starts_with_any(&["hi", "ho"]));
        assert!(String::from("test").starts_with_any(&["te"]));
    }

    #[test]
    fn test_ends_with_any_ext() {
        assert!("hello".ends_with_any(&["lo", "lo"]));
        assert!(!"hello".ends_with_any(&["hi", "ho"]));
        assert!(String::from("test").ends_with_any(&["st"]));
    }
}
