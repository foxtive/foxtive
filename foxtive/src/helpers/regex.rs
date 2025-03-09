/// A utility struct for working with regular expressions for username validation.
pub struct Regex;

/// Enum to specify case-sensitivity and character transformation rules.
pub enum CaseSensitivity {
    CaseSensitive,
    CaseInsensitive,
}

/// Enum representing different types of regex patterns for username validation.
pub enum RegexType {
    /// Allows only lowercase letters (1 to 38 characters).
    Alphabetic(CaseSensitivity),

    /// Allows only lowercase letters and numbers (1 to 38 characters), also must start with char.
    AlphaNumeric(CaseSensitivity),

    /// Allows lowercase letters, digits, and hyphens (`-`), but no consecutive or trailing hyphens.
    AlphaNumericDash(CaseSensitivity),

    /// Allows lowercase letters, digits, and dots (`.`), but no consecutive or trailing dots.
    AlphaNumericDot(CaseSensitivity),

    /// Allows lowercase letters, digits, hyphens (`-`), and dots (`.`), but no consecutive dots, hyphens, or trailing dots.
    AlphaNumericDashDot(CaseSensitivity),

    /// Allows lowercase letters, digits, and underscores (`_`), but no consecutive or trailing underscores.
    AlphaNumericUnderscore(CaseSensitivity),

    /// Allows lowercase letters, digits, dots (`.`), and underscores (`_`), but no consecutive dots or underscores, and no trailing dots or underscores.
    AlphaNumericDotUnderscore(CaseSensitivity),

    /// Provide your custom regex
    Custom(&'static str, Option<CaseSensitivity>),
}

impl Regex {
    /// Validates a string using a specified regex pattern.
    ///
    /// # Parameters
    /// - `val`: A string slice (`&str`) representing the value to validate.
    /// - `rt`: The `RegexType` enum variant that defines which regex pattern to use for validation.
    ///
    /// # Returns
    /// A `Box<Result<bool, fancy_regex::Error>>`, where:
    /// - `Ok(true)` means the string matches the regex pattern (valid username).
    /// - `Ok(false)` means the string does not match the regex pattern (invalid username).
    /// - `Err(fancy_regex::Error)` means there was an error compiling or executing the regex.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use foxtive::helpers::{CaseSensitivity, Regex, RegexType};
    ///
    /// let valid_username = "user_name123";
    /// let result = Regex::validate(valid_username, RegexType::AlphaNumericUnderscore(CaseSensitivity::CaseSensitive));
    /// assert_eq!(result.is_ok() && result.unwrap(), true);
    ///
    /// let invalid_username = "user@@";
    /// let result = Regex::validate(invalid_username, RegexType::AlphaNumeric(CaseSensitivity::CaseSensitive));
    /// assert_eq!(result.is_ok() && result.unwrap(), false);
    /// ```
    pub fn validate(val: &str, rt: RegexType) -> Box<Result<bool, fancy_regex::Error>> {
        let (regex_pattern, case_sensitivity) = Regex::acquire_regex(rt);

        // Adjust the regex pattern for case-insensitivity if necessary
        let regex_pattern = match case_sensitivity {
            CaseSensitivity::CaseInsensitive => format!("(?i){}", regex_pattern),
            _ => regex_pattern.to_string(),
        };

        let regex = match fancy_regex::Regex::new(&regex_pattern) {
            Ok(regex) => regex,
            Err(err) => return Box::new(Err(err)),
        };

        Box::new(regex.is_match(val))
    }

    /// Validates a username using a specified regex type. This method accepts a `Cow<str>` so it can handle both
    /// `&str` and `String` inputs.
    ///
    /// # Parameters
    /// - `val`: A `Cow<str>` representing the value to validate (can be either a string slice or a `String`).
    /// - `rt`: The `RegexType` enum variant that defines which regex pattern to use for validation.
    ///
    /// # Returns
    /// A `Box<Result<bool, fancy_regex::Error>>` indicating whether the username is valid according to the specified regex.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use foxtive::helpers::Regex;
    ///
    /// let valid_username = "user.first";
    /// let result = Regex::validate_username(valid_username);
    /// assert_eq!(result.is_ok() && result.unwrap(), true);
    ///
    /// let invalid_username = "user#123";
    /// let result = Regex::validate_username(invalid_username);
    /// assert_eq!(result.is_ok() && result.unwrap(), false);
    /// ```
    pub fn validate_username(val: &str) -> Box<Result<bool, fancy_regex::Error>> {
        Self::validate(
            val,
            RegexType::AlphaNumericDot(CaseSensitivity::CaseSensitive),
        )
    }

    /// Retrieves the regex pattern associated with the given `RegexType` variant.
    ///
    /// # Parameters
    /// - `rt`: The `RegexType` enum variant.
    ///
    /// # Returns
    /// A string slice (`&'static str`) containing the regex pattern associated with the `RegexType` variant.
    fn acquire_regex(rt: RegexType) -> (&'static str, CaseSensitivity) {
        match rt {
            RegexType::Alphabetic(cs) => (r"^[a-z]{1,38}$", cs), // Only lowercase letters, 1-38 characters.
            RegexType::AlphaNumeric(cs) => (r"^[a-z][a-z0-9]{0,37}$", cs), // Only lowercase letters, 1-38 characters.
            RegexType::AlphaNumericDash(cs) => (r"^[a-z](?!.*\-\-)(?!.*\-$)[a-z\d\-]{0,37}$", cs), // Letters, digits, and dashes, no consecutive or trailing dashes.
            RegexType::AlphaNumericDot(cs) => (r"^[a-z](?!.*\.\.)(?!.*\.$)[a-z\d\.]{0,37}$", cs), // Letters, digits, and dots, no consecutive or trailing dots.
            RegexType::AlphaNumericUnderscore(cs) => {
                (r"^[a-z](?!.*\_\_)(?!.*\_$)[a-z\d\_]{0,37}$", cs)
            } // Letters, digits, and underscores, no consecutive or trailing underscores.
            RegexType::AlphaNumericDotUnderscore(cs) => {
                (r"^[a-z](?!.*\.\.)(?!.*\.$)[a-z\d\._]{0,37}$", cs)
            } // Letters, digits, dots, and underscores.
            RegexType::AlphaNumericDashDot(cs) => (
                r"^[a-z](?!.*\-\-)(?!.*\.\.)(?!.*\-$)(?!.*\.$)[a-z\d\-\.\_]{0,37}$",
                cs,
            ), // Letters, digits, dashes, dots, and underscores.
            RegexType::Custom(val, cs) => (val, cs.unwrap_or(CaseSensitivity::CaseSensitive)),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    // Test for Alphabetic regex type
    #[test]
    fn test_alphabetic_valid() {
        // Case-sensitive tests
        let result = Regex::validate(
            "username",
            RegexType::Alphabetic(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && result.unwrap());

        let result = Regex::validate(
            "USERNAME",
            RegexType::Alphabetic(CaseSensitivity::CaseInsensitive),
        );
        assert!(result.is_ok() && result.unwrap());
    }

    #[test]
    fn test_alphabetic_case_insensitive_valid() {
        let result = Regex::validate(
            "UserName",
            RegexType::Alphabetic(CaseSensitivity::CaseInsensitive),
        );
        assert!(result.is_ok() && result.unwrap());
    }

    #[test]
    fn test_alphabetic_invalid() {
        // Case-sensitive tests
        let result = Regex::validate(
            "user1name",
            RegexType::Alphabetic(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());

        let result = Regex::validate(
            "1username",
            RegexType::Alphabetic(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());

        let result = Regex::validate(
            "username1",
            RegexType::Alphabetic(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());
    }

    // Test for AlphaNumeric regex type
    #[test]
    fn test_alpha_numeric_valid() {
        // Case-sensitive tests
        let result = Regex::validate(
            "username",
            RegexType::AlphaNumeric(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && result.unwrap());

        let result = Regex::validate(
            "user1name",
            RegexType::AlphaNumeric(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && result.unwrap());

        let result = Regex::validate(
            "user1name2",
            RegexType::AlphaNumeric(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && result.unwrap());
    }

    #[test]
    fn test_alpha_numeric_case_insensitive_valid() {
        let result = Regex::validate(
            "User1Name",
            RegexType::AlphaNumeric(CaseSensitivity::CaseInsensitive),
        );
        assert!(result.is_ok() && result.unwrap());
    }

    #[test]
    fn test_alpha_numeric_invalid() {
        let result = Regex::validate(
            "user123!",
            RegexType::AlphaNumeric(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());

        let result = Regex::validate(
            "user123_",
            RegexType::AlphaNumeric(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());

        let result = Regex::validate(
            "123user",
            RegexType::AlphaNumeric(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());
    }

    // Test for AlphaNumericDash regex type
    #[test]
    fn test_alpha_numeric_dash_valid() {
        // Case-sensitive tests
        let result = Regex::validate(
            "username-123",
            RegexType::AlphaNumericDash(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && result.unwrap());

        let result = Regex::validate(
            "user-123",
            RegexType::AlphaNumericDash(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && result.unwrap());
    }

    #[test]
    fn test_alpha_numeric_dash_case_insensitive_valid() {
        let result = Regex::validate(
            "UserName-123",
            RegexType::AlphaNumericDash(CaseSensitivity::CaseInsensitive),
        );
        assert!(result.is_ok() && result.unwrap());
    }

    #[test]
    fn test_alpha_numeric_dash_invalid() {
        let result = Regex::validate(
            "user--123",
            RegexType::AlphaNumericDash(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());

        let result = Regex::validate(
            "user-",
            RegexType::AlphaNumericDash(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());

        let result = Regex::validate(
            "-user123",
            RegexType::AlphaNumericDash(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());
    }

    // Test for AlphaNumericDot regex type
    #[test]
    fn test_alpha_numeric_dot_valid() {
        // Case-sensitive tests
        let result = Regex::validate(
            "user.name",
            RegexType::AlphaNumericDot(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && result.unwrap());

        let result = Regex::validate(
            "user123.name",
            RegexType::AlphaNumericDot(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && result.unwrap());
    }

    #[test]
    fn test_alpha_numeric_dot_case_insensitive_valid() {
        let result = Regex::validate(
            "User.Name",
            RegexType::AlphaNumericDot(CaseSensitivity::CaseInsensitive),
        );
        assert!(result.is_ok() && result.unwrap());
    }

    #[test]
    fn test_alpha_numeric_dot_invalid() {
        let result = Regex::validate(
            "user..name",
            RegexType::AlphaNumericDot(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());

        let result = Regex::validate(
            "user.name.",
            RegexType::AlphaNumericDot(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());

        let result = Regex::validate(
            ".username",
            RegexType::AlphaNumericDot(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());
    }

    // Test for AlphaNumericDashDot regex type
    #[test]
    fn test_alpha_numeric_dash_dot_valid() {
        // Case-sensitive tests
        let result = Regex::validate(
            "user-name.123",
            RegexType::AlphaNumericDashDot(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && result.unwrap());

        let result = Regex::validate(
            "user-name_123",
            RegexType::AlphaNumericDashDot(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && result.unwrap());
    }

    #[test]
    fn test_alpha_numeric_dash_dot_case_insensitive_valid() {
        let result = Regex::validate(
            "User-Name.123",
            RegexType::AlphaNumericDashDot(CaseSensitivity::CaseInsensitive),
        );
        assert!(result.is_ok() && result.unwrap());
    }

    #[test]
    fn test_alpha_numeric_dash_dot_invalid() {
        let result = Regex::validate(
            "user..name",
            RegexType::AlphaNumericDashDot(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());

        let result = Regex::validate(
            "user-name.",
            RegexType::AlphaNumericDashDot(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());

        let result = Regex::validate(
            "user-.name",
            RegexType::AlphaNumericDashDot(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && result.unwrap());
    }

    // Test for AlphaNumericUnderscore regex type
    #[test]
    fn test_alpha_numeric_underscore_valid() {
        // Case-sensitive tests
        let result = Regex::validate(
            "user_name",
            RegexType::AlphaNumericUnderscore(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && result.unwrap());

        let result = Regex::validate(
            "user123_name",
            RegexType::AlphaNumericUnderscore(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && result.unwrap());
    }

    #[test]
    fn test_alpha_numeric_underscore_case_insensitive_valid() {
        let result = Regex::validate(
            "User_Name",
            RegexType::AlphaNumericUnderscore(CaseSensitivity::CaseInsensitive),
        );
        assert!(result.is_ok() && result.unwrap());
    }

    #[test]
    fn test_alpha_numeric_underscore_invalid() {
        let result = Regex::validate(
            "user__name",
            RegexType::AlphaNumericUnderscore(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());

        let result = Regex::validate(
            "user_name_",
            RegexType::AlphaNumericUnderscore(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());

        let result = Regex::validate(
            "_username",
            RegexType::AlphaNumericUnderscore(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());
    }

    // Test for AlphaNumericDotUnderscore regex type
    #[test]
    fn test_alpha_numeric_dot_underscore_valid() {
        // Case-sensitive tests
        let result = Regex::validate(
            "user.name_123",
            RegexType::AlphaNumericDotUnderscore(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && result.unwrap());
    }

    #[test]
    fn test_alpha_numeric_dot_underscore_case_insensitive_valid() {
        let result = Regex::validate(
            "User.Name_123",
            RegexType::AlphaNumericDotUnderscore(CaseSensitivity::CaseInsensitive),
        );
        assert!(result.is_ok() && result.unwrap());
    }

    #[test]
    fn test_alpha_numeric_dot_underscore_invalid() {
        let result = Regex::validate(
            "user..name_123",
            RegexType::AlphaNumericDotUnderscore(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());

        let result = Regex::validate(
            "user_name_.",
            RegexType::AlphaNumericDotUnderscore(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());

        let result = Regex::validate(
            "_user.name",
            RegexType::AlphaNumericDotUnderscore(CaseSensitivity::CaseSensitive),
        );
        assert!(result.is_ok() && !result.unwrap());
    }
}
