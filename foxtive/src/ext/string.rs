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
    fn slugify(&self) -> String;
    fn snake_case(&self) -> String;
    fn kebab_case(&self) -> String;
    fn pascal_case(&self) -> String;
    fn title_case(&self) -> String;
    fn is_email(&self) -> bool;
    fn is_url(&self) -> bool;
    fn is_uuid(&self) -> bool;
    fn words(&self) -> Vec<&str>;
    fn lines(&self) -> Vec<&str>;
    fn repeat_str(&self, n: usize) -> String;
    fn starts_with_any(&self, prefixes: &[&str]) -> bool;
    fn ends_with_any(&self, suffixes: &[&str]) -> bool;
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

    fn slugify(&self) -> String {
        StringHelper::slugify(self)
    }

    fn snake_case(&self) -> String {
        StringHelper::snake_case(self)
    }

    fn kebab_case(&self) -> String {
        StringHelper::kebab_case(self)
    }

    fn pascal_case(&self) -> String {
        StringHelper::pascal_case(self)
    }

    fn title_case(&self) -> String {
        StringHelper::title_case(self)
    }

    fn is_email(&self) -> bool {
        StringHelper::is_email(self)
    }

    fn is_url(&self) -> bool {
        StringHelper::is_url(self)
    }

    fn is_uuid(&self) -> bool {
        StringHelper::is_uuid(self)
    }

    fn words(&self) -> Vec<&str> {
        StringHelper::words(self)
    }

    fn lines(&self) -> Vec<&str> {
        StringHelper::lines(self)
    }

    fn repeat_str(&self, n: usize) -> String {
        StringHelper::repeat(self, n)
    }

    fn starts_with_any(&self, prefixes: &[&str]) -> bool {
        StringHelper::starts_with_any(self, prefixes)
    }

    fn ends_with_any(&self, suffixes: &[&str]) -> bool {
        StringHelper::ends_with_any(self, suffixes)
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

    fn slugify(&self) -> String {
        self.as_str().slugify()
    }

    fn snake_case(&self) -> String {
        self.as_str().snake_case()
    }

    fn kebab_case(&self) -> String {
        self.as_str().kebab_case()
    }

    fn pascal_case(&self) -> String {
        self.as_str().pascal_case()
    }

    fn title_case(&self) -> String {
        self.as_str().title_case()
    }

    fn is_email(&self) -> bool {
        self.as_str().is_email()
    }

    fn is_url(&self) -> bool {
        self.as_str().is_url()
    }

    fn is_uuid(&self) -> bool {
        self.as_str().is_uuid()
    }

    fn words(&self) -> Vec<&str> {
        self.as_str().words()
    }

    fn lines(&self) -> Vec<&str> {
        StringHelper::lines(self)
    }

    fn repeat_str(&self, n: usize) -> String {
        self.as_str().repeat_str(n)
    }

    fn starts_with_any(&self, prefixes: &[&str]) -> bool {
        self.as_str().starts_with_any(prefixes)
    }

    fn ends_with_any(&self, suffixes: &[&str]) -> bool {
        self.as_str().ends_with_any(suffixes)
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

    #[test]
    fn slugify_creates_url_friendly_string() {
        assert_eq!("Hello World".slugify(), "hello-world");
        assert_eq!("Rust is great!".slugify(), "rust-is-great");
        assert_eq!("".slugify(), "");
        assert_eq!(String::from("Foo Bar").slugify(), "foo-bar");
    }

    #[test]
    fn snake_case_converts_correctly() {
        assert_eq!("camelCase".snake_case(), "camel_case");
        assert_eq!("PascalCase".snake_case(), "pascal_case");
        assert_eq!("kebab-case".snake_case(), "kebab_case");
        assert_eq!("hello world".snake_case(), "hello_world");
        assert_eq!(String::from("HTMLParser").snake_case(), "html_parser");
    }

    #[test]
    fn kebab_case_converts_correctly() {
        assert_eq!("camelCase".kebab_case(), "camel-case");
        assert_eq!("snake_case".kebab_case(), "snake-case");
        assert_eq!("hello world".kebab_case(), "hello-world");
        assert_eq!(String::from("PascalCase").kebab_case(), "pascal-case");
    }

    #[test]
    fn pascal_case_converts_correctly() {
        assert_eq!("snake_case".pascal_case(), "SnakeCase");
        assert_eq!("kebab-case".pascal_case(), "KebabCase");
        assert_eq!("hello world".pascal_case(), "HelloWorld");
        assert_eq!(String::from("camelCase").pascal_case(), "CamelCase");
    }

    #[test]
    fn title_case_converts_correctly() {
        assert_eq!("hello world".title_case(), "Hello World");
        assert_eq!(
            String::from("rust programming").title_case(),
            "Rust Programming"
        );
    }

    #[test]
    fn is_email_validates_correctly() {
        assert!("user@example.com".is_email());
        assert!("test.user@domain.org".is_email());
        assert!(!"".is_email());
        assert!(!"invalid".is_email());
        assert!(!"@domain.com".is_email());
        assert!(String::from("a@b.c").is_email());
    }

    #[test]
    fn is_url_validates_correctly() {
        assert!("http://example.com".is_url());
        assert!("https://example.com".is_url());
        assert!(!"".is_url());
        assert!(!"example.com".is_url());
        assert!(String::from("https://test.org/path").is_url());
    }

    #[test]
    fn is_uuid_validates_correctly() {
        assert!("550e8400-e29b-41d4-a716-446655440000".is_uuid());
        assert!(!"".is_uuid());
        assert!(!"not-a-uuid".is_uuid());
        assert!(String::from("6ba7b810-9dad-11d1-80b4-00c04fd430c8").is_uuid());
    }

    #[test]
    fn words_splits_correctly() {
        assert_eq!("hello world".words(), vec!["hello", "world"]);
        assert_eq!("  multiple   spaces  ".words(), vec!["multiple", "spaces"]);
        assert_eq!("".words(), Vec::<&str>::new());
        assert_eq!(String::from("single").words(), vec!["single"]);
    }

    #[test]
    fn lines_splits_correctly() {
        assert_eq!(
            StringExt::lines("line1\nline2\nline3"),
            vec!["line1", "line2", "line3"]
        );
        assert_eq!(StringExt::lines("single"), vec!["single"]);
        assert_eq!(String::from("a\nb").lines(), vec!["a", "b"]);
    }

    #[test]
    fn repeat_str_repeats_correctly() {
        assert_eq!("abc".repeat_str(3), "abcabcabc");
        assert_eq!("x".repeat_str(5), "xxxxx");
        assert_eq!("hello".repeat_str(0), "");
        assert_eq!(String::from("ab").repeat_str(2), "abab");
    }

    #[test]
    fn starts_with_any_checks_correctly() {
        assert!("hello world".starts_with_any(&["hello", "hi"]));
        assert!("hi there".starts_with_any(&["hello", "hi"]));
        assert!(!"hey".starts_with_any(&["hello", "hi"]));
        assert!(String::from("test").starts_with_any(&["te"]));
    }

    #[test]
    fn ends_with_any_checks_correctly() {
        assert!("hello world".ends_with_any(&["world", "earth"]));
        assert!("hello earth".ends_with_any(&["world", "earth"]));
        assert!(!"hello".ends_with_any(&["world", "earth"]));
        assert!(String::from("test").ends_with_any(&["st"]));
    }
}
