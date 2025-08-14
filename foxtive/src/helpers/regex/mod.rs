mod text_cleaner;
mod tester;

pub use text_cleaner::TextCleaner;
pub use tester::*;

/// Enum to specify case-sensitivity and character transformation rules.
#[derive(Clone, Copy)]
pub enum CaseSensitivity {
    CaseSensitive,
    CaseInsensitive,
}

/// Enum representing different types of cleaning patterns for text sanitization.
#[derive(Clone)]
pub enum RegexType {
    /// Keeps only lowercase letters, removes everything else (max 38 chars).
    Alphabetic(CaseSensitivity),

    /// Keeps only letters and numbers, ensures it starts with a letter (max 38 chars).
    AlphaNumeric(CaseSensitivity),

    /// Keeps letters, digits, and hyphens, removes consecutive/trailing hyphens.
    AlphaNumericDash(CaseSensitivity),

    /// Keeps letters, digits, and dots, removes consecutive/trailing dots.
    AlphaNumericDot(CaseSensitivity),

    /// Keeps letters, digits, hyphens, and dots, removes consecutive/trailing special chars.
    AlphaNumericDashDot(CaseSensitivity),

    /// Keeps letters, digits, and underscores, removes consecutive/trailing underscores.
    AlphaNumericUnderscore(CaseSensitivity),

    /// Keeps letters, digits, dots, and underscores, removes consecutive/trailing special chars.
    AlphaNumericDotUnderscore(CaseSensitivity),

    /// Use a custom cleaning pattern (allowed_chars, max_length)
    Custom(&'static str, Option<CaseSensitivity>, usize),
}
