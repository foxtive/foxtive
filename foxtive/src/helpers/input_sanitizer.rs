#[cfg(feature = "html-sanitizer")]
pub use ammonia;

pub struct InputSanitizer;

impl InputSanitizer {
    pub fn sanitize_filename(input: &str) -> String {
        input.chars()
            .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
            .collect()
    }

    #[cfg(feature = "html-sanitizer")]
    pub fn sanitize_html(input: &str) -> String {
        ammonia::clean(input)
    }
}