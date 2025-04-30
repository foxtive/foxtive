use crate::helpers::string::Str;

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
        Str::uc_first(self)
    }

    fn uc_words(&self) -> String {
        Str::uc_words(self)
    }

    #[cfg(feature = "regex")]
    fn is_username_valid(&self) -> Box<fancy_regex::Result<bool>> {
        // Str::is_username_valid takes String as param
        Str::is_username_valid(self.to_string())
    }

    fn truncate(&self, max_length: usize) -> String {
        Str::truncate(self, max_length)
    }

    fn remove_whitespace(&self) -> String {
        Str::remove_whitespace(self)
    }

    fn reverse(&self) -> String {
        Str::reverse(self)
    }

    fn count_occurrences(&self, substr: &str) -> usize {
        Str::count_occurrences(self, substr)
    }

    fn is_numeric(&self) -> bool {
        Str::is_numeric(self)
    }

    fn is_alphabetic(&self) -> bool {
        Str::is_alphabetic(self)
    }

    fn camel_case(&self) -> String {
        Str::camel_case(self)
    }

    fn pad_left(&self, width: usize, pad_char: char) -> String {
        Str::pad_left(self, width, pad_char)
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

