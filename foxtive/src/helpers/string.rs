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
        assert!(!Str::is_username_valid(
            "a.b.c.d.e.f.g.h.i.j.k.l.m.n.o.p.q.r.s.t.u.v.w.x.y.z".to_string()
        )
        .unwrap()); // More than 37 chars
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
}
