use crate::helpers::regex::{CaseSensitivity, RegexType};

/// A utility struct for cleaning text using regex patterns for username validation.
pub struct TextCleaner;

impl TextCleaner {
    /// Cleans a string according to the specified cleaning rules.
    ///
    /// # Parameters
    /// - `text`: A string slice (`&str`) representing the text to clean.
    /// - `cleaning_type`: The `RegexType` enum variant that defines how to clean the text.
    ///
    /// # Returns
    /// A `String` containing the cleaned text that conforms to the specified pattern.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use foxtive::helpers::regex::{CaseSensitivity, TextCleaner, RegexType};
    ///
    /// let dirty_text = "User@@Name123!!";
    /// let cleaned = TextCleaner::clean(dirty_text, RegexType::AlphaNumeric(CaseSensitivity::CaseInsensitive));
    /// assert_eq!(cleaned, "username123");
    ///
    /// let text_with_dots = "user..name..123";
    /// let cleaned = TextCleaner::clean(text_with_dots, RegexType::AlphaNumericDot(CaseSensitivity::CaseSensitive));
    /// assert_eq!(cleaned, "user.name.123");
    /// ```
    pub fn clean(text: &str, cleaning_type: RegexType) -> String {
        match cleaning_type {
            RegexType::Alphabetic(case_sensitivity) => {
                Self::clean_alphabetic(text, case_sensitivity)
            }
            RegexType::AlphaNumeric(case_sensitivity) => {
                Self::clean_alphanumeric(text, case_sensitivity)
            }
            RegexType::AlphaNumericDash(case_sensitivity) => {
                Self::clean_alphanumeric_dash(text, case_sensitivity)
            }
            RegexType::AlphaNumericDot(case_sensitivity) => {
                Self::clean_alphanumeric_dot(text, case_sensitivity)
            }
            RegexType::AlphaNumericDashDot(case_sensitivity) => {
                Self::clean_alphanumeric_dash_dot(text, case_sensitivity)
            }
            RegexType::AlphaNumericUnderscore(case_sensitivity) => {
                Self::clean_alphanumeric_underscore(text, case_sensitivity)
            }
            RegexType::AlphaNumericDotUnderscore(case_sensitivity) => {
                Self::clean_alphanumeric_dot_underscore(text, case_sensitivity)
            }
            RegexType::Custom(allowed_chars, case_sensitivity, max_length) => {
                Self::clean_custom(text, allowed_chars, case_sensitivity.unwrap_or(CaseSensitivity::CaseSensitive), max_length)
            }
        }
    }

    /// Cleans text for username format (AlphaNumericDot with case sensitivity).
    ///
    /// # Parameters
    /// - `text`: A string slice (`&str`) representing the text to clean.
    ///
    /// # Returns
    /// A `String` containing the cleaned username.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use foxtive::helpers::regex::TextCleaner;
    ///
    /// let dirty_username = "User..First@@123";
    /// let cleaned = TextCleaner::clean_username(dirty_username);
    /// assert_eq!(cleaned, "user.first123");
    /// ```
    pub fn clean_username(text: &str) -> String {
        Self::clean(text, RegexType::AlphaNumericDot(CaseSensitivity::CaseSensitive))
    }

    /// Cleans text to contain only alphabetic characters.
    fn clean_alphabetic(text: &str, case_sensitivity: CaseSensitivity) -> String {
        let mut result: String = text.chars()
            .filter(|c| c.is_alphabetic())
            .collect();

        result = Self::apply_case_transformation(result, case_sensitivity);
        Self::truncate_to_length(result, 38)
    }

    /// Cleans text to contain only alphanumeric characters, ensuring it starts with a letter.
    fn clean_alphanumeric(text: &str, case_sensitivity: CaseSensitivity) -> String {
        let mut result: String = text.chars()
            .filter(|c| c.is_alphanumeric())
            .collect();

        result = Self::apply_case_transformation(result, case_sensitivity);
        result = Self::ensure_starts_with_letter(result);
        Self::truncate_to_length(result, 38)
    }

    /// Cleans text for alphanumeric + dash pattern.
    fn clean_alphanumeric_dash(text: &str, case_sensitivity: CaseSensitivity) -> String {
        let mut result: String = text.chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect();

        result = Self::apply_case_transformation(result, case_sensitivity);
        result = Self::ensure_starts_with_letter(result);
        result = Self::remove_consecutive_chars(result, '-');
        result = Self::remove_trailing_char(result, '-');
        Self::truncate_to_length(result, 38)
    }

    /// Cleans text for alphanumeric + dot pattern.
    fn clean_alphanumeric_dot(text: &str, case_sensitivity: CaseSensitivity) -> String {
        let mut result: String = text.chars()
            .filter(|c| c.is_alphanumeric() || *c == '.')
            .collect();

        result = Self::apply_case_transformation(result, case_sensitivity);
        result = Self::ensure_starts_with_letter(result);
        result = Self::remove_consecutive_chars(result, '.');
        result = Self::remove_trailing_char(result, '.');
        Self::truncate_to_length(result, 38)
    }

    /// Cleans text for alphanumeric + dash + dot pattern.
    fn clean_alphanumeric_dash_dot(text: &str, case_sensitivity: CaseSensitivity) -> String {
        let mut result: String = text.chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '.' || *c == '_')
            .collect();

        result = Self::apply_case_transformation(result, case_sensitivity);
        result = Self::ensure_starts_with_letter(result);
        result = Self::remove_consecutive_chars(result, '.');
        result = Self::remove_consecutive_chars(result, '-');
        result = Self::remove_trailing_char(result, '.');
        result = Self::remove_trailing_char(result, '-');
        Self::truncate_to_length(result, 38)
    }

    /// Cleans text for alphanumeric + underscore pattern.
    fn clean_alphanumeric_underscore(text: &str, case_sensitivity: CaseSensitivity) -> String {
        let mut result: String = text.chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect();

        result = Self::apply_case_transformation(result, case_sensitivity);
        result = Self::ensure_starts_with_letter(result);
        result = Self::remove_consecutive_chars(result, '_');
        result = Self::remove_trailing_char(result, '_');
        Self::truncate_to_length(result, 38)
    }

    /// Cleans text for alphanumeric + dot + underscore pattern.
    fn clean_alphanumeric_dot_underscore(text: &str, case_sensitivity: CaseSensitivity) -> String {
        let mut result: String = text.chars()
            .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '_')
            .collect();

        result = Self::apply_case_transformation(result, case_sensitivity);
        result = Self::ensure_starts_with_letter(result);
        result = Self::remove_consecutive_chars(result, '.');
        result = Self::remove_consecutive_chars(result, '_');
        result = Self::remove_trailing_char(result, '.');
        result = Self::remove_trailing_char(result, '_');
        Self::truncate_to_length(result, 38)
    }

    /// Cleans text using custom allowed characters.
    fn clean_custom(text: &str, allowed_chars: &str, case_sensitivity: CaseSensitivity, max_length: usize) -> String {
        let allowed_set: std::collections::HashSet<char> = allowed_chars.chars().collect();

        let mut result: String = text.chars()
            .filter(|c| allowed_set.contains(c) || c.is_alphanumeric())
            .collect();

        result = Self::apply_case_transformation(result, case_sensitivity);
        Self::truncate_to_length(result, max_length)
    }

    /// Applies case transformation based on sensitivity setting.
    fn apply_case_transformation(text: String, case_sensitivity: CaseSensitivity) -> String {
        match case_sensitivity {
            CaseSensitivity::CaseSensitive => text.to_lowercase(),
            CaseSensitivity::CaseInsensitive => text.to_lowercase(),
        }
    }

    /// Ensures the string starts with a letter, removing leading non-letters.
    fn ensure_starts_with_letter(text: String) -> String {
        text.chars()
            .skip_while(|c| !c.is_alphabetic())
            .collect()
    }

    /// Removes consecutive occurrences of a specific character.
    fn remove_consecutive_chars(text: String, target_char: char) -> String {
        let mut result = String::new();
        let mut prev_char = None;

        for ch in text.chars() {
            if ch == target_char && prev_char == Some(target_char) {
                continue; // Skip consecutive target characters
            }
            result.push(ch);
            prev_char = Some(ch);
        }

        result
    }

    /// Removes trailing occurrences of a specific character.
    fn remove_trailing_char(text: String, target_char: char) -> String {
        text.trim_end_matches(target_char).to_string()
    }

    /// Truncates string to specified maximum length.
    fn truncate_to_length(text: String, max_length: usize) -> String {
        text.chars().take(max_length).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_alphabetic() {
        let dirty_text = "User123Name!!!";
        let cleaned = TextCleaner::clean(dirty_text, RegexType::Alphabetic(CaseSensitivity::CaseSensitive));
        assert_eq!(cleaned, "username");

        let mixed_case = "UserNAME";
        let cleaned = TextCleaner::clean(mixed_case, RegexType::Alphabetic(CaseSensitivity::CaseInsensitive));
        assert_eq!(cleaned, "username");
    }

    #[test]
    fn test_clean_alphanumeric() {
        let dirty_text = "123User@@Name456";
        let cleaned = TextCleaner::clean(dirty_text, RegexType::AlphaNumeric(CaseSensitivity::CaseSensitive));
        assert_eq!(cleaned, "username456");

        let starts_with_number = "123username";
        let cleaned = TextCleaner::clean(starts_with_number, RegexType::AlphaNumeric(CaseSensitivity::CaseSensitive));
        assert_eq!(cleaned, "username");
    }

    #[test]
    fn test_clean_alphanumeric_dash() {
        let dirty_text = "user--name@@123";
        let cleaned = TextCleaner::clean(dirty_text, RegexType::AlphaNumericDash(CaseSensitivity::CaseSensitive));
        assert_eq!(cleaned, "user-name123");

        let trailing_dash = "username-";
        let cleaned = TextCleaner::clean(trailing_dash, RegexType::AlphaNumericDash(CaseSensitivity::CaseSensitive));
        assert_eq!(cleaned, "username");
    }

    #[test]
    fn test_clean_alphanumeric_dot() {
        let dirty_text = "user..name@@123";
        let cleaned = TextCleaner::clean(dirty_text, RegexType::AlphaNumericDot(CaseSensitivity::CaseSensitive));
        assert_eq!(cleaned, "user.name123");

        let trailing_dot = "username.";
        let cleaned = TextCleaner::clean(trailing_dot, RegexType::AlphaNumericDot(CaseSensitivity::CaseSensitive));
        assert_eq!(cleaned, "username");
    }

    #[test]
    fn test_clean_alphanumeric_underscore() {
        let dirty_text = "user__name@@123";
        let cleaned = TextCleaner::clean(dirty_text, RegexType::AlphaNumericUnderscore(CaseSensitivity::CaseSensitive));
        assert_eq!(cleaned, "user_name123");

        let trailing_underscore = "username_";
        let cleaned = TextCleaner::clean(trailing_underscore, RegexType::AlphaNumericUnderscore(CaseSensitivity::CaseSensitive));
        assert_eq!(cleaned, "username");
    }

    #[test]
    fn test_clean_username() {
        let dirty_username = "User..First@@123";
        let cleaned = TextCleaner::clean_username(dirty_username);
        assert_eq!(cleaned, "user.first123");

        let complex_username = "!!!User123..Name456...";
        let cleaned = TextCleaner::clean_username(complex_username);
        assert_eq!(cleaned, "user123.name456");
    }

    #[test]
    fn test_clean_custom() {
        let dirty_text = "user@domain.com";
        let cleaned = TextCleaner::clean(dirty_text, RegexType::Custom("@.", Some(CaseSensitivity::CaseSensitive), 20));
        assert_eq!(cleaned, "user@domain.com");

        let long_text = "a".repeat(50);
        let cleaned = TextCleaner::clean(&long_text, RegexType::Custom("", Some(CaseSensitivity::CaseSensitive), 10));
        assert_eq!(cleaned.len(), 10);
    }

    #[test]
    fn test_length_truncation() {
        let long_text = "a".repeat(50);
        let cleaned = TextCleaner::clean(&long_text, RegexType::Alphabetic(CaseSensitivity::CaseSensitive));
        assert_eq!(cleaned.len(), 38);
    }

    #[test]
    fn test_ensure_starts_with_letter() {
        let starts_with_number = "123abc";
        let cleaned = TextCleaner::clean(starts_with_number, RegexType::AlphaNumeric(CaseSensitivity::CaseSensitive));
        assert_eq!(cleaned, "abc");

        let starts_with_symbol = "___abc123";
        let cleaned = TextCleaner::clean(starts_with_symbol, RegexType::AlphaNumericUnderscore(CaseSensitivity::CaseSensitive));
        assert_eq!(cleaned, "abc123");
    }

    #[test]
    fn test_consecutive_character_removal() {
        let multiple_dots = "user...name";
        let cleaned = TextCleaner::clean(multiple_dots, RegexType::AlphaNumericDot(CaseSensitivity::CaseSensitive));
        assert_eq!(cleaned, "user.name");

        let multiple_underscores = "user___name";
        let cleaned = TextCleaner::clean(multiple_underscores, RegexType::AlphaNumericUnderscore(CaseSensitivity::CaseSensitive));
        assert_eq!(cleaned, "user_name");
    }
}