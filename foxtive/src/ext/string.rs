use crate::helpers::string::StringHelper;

pub trait StringExt {
    fn uc_first(&self) -> String;
    fn uc_words(&self) -> String;
    #[cfg(feature = "regex")]
    fn is_username_valid(&self) -> Box<fancy_regex::Result<bool>>;
    fn truncate(&self, max_length: usize) -> String;
    fn remove_whitespace(&self) -> String;
    fn reverse(&self) -> String;
    fn count_occurrences(&self, substr: &str) -> usize;
    fn is_numeric(&self) -> bool;
    fn is_alphabetic(&self) -> bool;
    fn camel_case(&self) -> String;
    fn pad_left(&self, width: usize, pad_char: char) -> String;
}

impl StringExt for str {
    fn uc_first(&self) -> String {
        StringHelper::uc_first(self)
    }

    fn uc_words(&self) -> String {
        StringHelper::uc_words(self)
    }

    #[cfg(feature = "regex")]
    fn is_username_valid(&self) -> Box<fancy_regex::Result<bool>> {
        // StringHelper::is_username_valid takes String as param
        StringHelper::is_username_valid(self.to_string())
    }

    fn truncate(&self, max_length: usize) -> String {
        StringHelper::truncate(self, max_length)
    }

    fn remove_whitespace(&self) -> String {
        StringHelper::remove_whitespace(self)
    }

    fn reverse(&self) -> String {
        StringHelper::reverse(self)
    }

    fn count_occurrences(&self, substr: &str) -> usize {
        StringHelper::count_occurrences(self, substr)
    }

    fn is_numeric(&self) -> bool {
        StringHelper::is_numeric(self)
    }

    fn is_alphabetic(&self) -> bool {
        StringHelper::is_alphabetic(self)
    }

    fn camel_case(&self) -> String {
        StringHelper::camel_case(self)
    }

    fn pad_left(&self, width: usize, pad_char: char) -> String {
        StringHelper::pad_left(self, width, pad_char)
    }
}

impl StringExt for String {
    fn uc_first(&self) -> String {
        self.as_str().uc_first()
    }

    fn uc_words(&self) -> String {
        self.as_str().uc_words()
    }

    #[cfg(feature = "regex")]
    fn is_username_valid(&self) -> Box<fancy_regex::Result<bool>> {
        self.as_str().is_username_valid()
    }

    fn truncate(&self, max_length: usize) -> String {
        self.as_str().truncate(max_length)
    }

    fn remove_whitespace(&self) -> String {
        self.as_str().remove_whitespace()
    }

    fn reverse(&self) -> String {
        self.as_str().reverse()
    }

    fn count_occurrences(&self, substr: &str) -> usize {
        self.as_str().count_occurrences(substr)
    }

    fn is_numeric(&self) -> bool {
        self.as_str().is_numeric()
    }

    fn is_alphabetic(&self) -> bool {
        self.as_str().is_alphabetic()
    }

    fn camel_case(&self) -> String {
        self.as_str().camel_case()
    }

    fn pad_left(&self, width: usize, pad_char: char) -> String {
        self.as_str().pad_left(width, pad_char)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uc_first_capitalizes_first_char() {
        assert_eq!("hello".uc_first(), "Hello");
        assert_eq!("Hello".uc_first(), "Hello");
        assert_eq!("".uc_first(), "");
    }

    #[test]
    fn truncate_shortens_string() {
        assert_eq!("hello world".truncate(5), "hello...");
        assert_eq!("hi".truncate(10), "hi");
    }

    #[test]
    fn remove_whitespace_strips_all_spaces() {
        assert_eq!("hello world".remove_whitespace(), "helloworld");
        assert_eq!("  spaces  ".remove_whitespace(), "spaces");
    }

    #[test]
    fn reverse_reverses_characters() {
        assert_eq!("hello".reverse(), "olleh");
        assert_eq!("".reverse(), "");
    }

    #[test]
    fn count_occurrences_counts_substrings() {
        assert_eq!("hello world hello".count_occurrences("hello"), 2);
        assert_eq!("abc".count_occurrences("z"), 0);
    }

    #[test]
    fn is_numeric_checks_digits() {
        assert!("12345".is_numeric());
        assert!(!"123abc".is_numeric());
        assert!(!"".is_numeric());
    }

    #[test]
    fn is_alphabetic_checks_letters() {
        assert!("abcDEF".is_alphabetic());
        assert!(!"abc123".is_alphabetic());
    }

    #[test]
    fn camel_case_converts_from_snake_case() {
        assert_eq!("hello_world".camel_case(), "helloWorld");
        assert_eq!("one_two_three".camel_case(), "oneTwoThree");
    }

    #[test]
    fn pad_left_pads_to_width() {
        assert_eq!("42".pad_left(5, '0'), "00042");
        assert_eq!("hello".pad_left(3, ' '), "hello");
    }

    #[test]
    fn string_type_delegates_to_str() {
        let s = String::from("hello");
        assert_eq!(s.uc_first(), "Hello");
        assert_eq!(s.reverse(), "olleh");
    }
}
