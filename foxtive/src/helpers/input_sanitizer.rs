#[cfg(feature = "html-sanitizer")]
pub use ammonia;

pub struct InputSanitizer;

impl InputSanitizer {
    /// Sanitize a filename by removing path separators and dangerous characters.
    ///
    /// - Strips directory components (only the final filename segment is kept)
    /// - Removes null bytes and control characters
    /// - Allows alphanumeric, dots, underscores, and hyphens
    /// - Collapses consecutive dots to prevent `..` traversal
    pub fn sanitize_filename(input: &str) -> String {
        // Take only the final path segment to prevent directory traversal
        let filename = input.rsplit(['/', '\\']).next().unwrap_or(input);

        let sanitized: String = filename
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
            .collect();

        // Collapse consecutive dots to prevent `..` traversal
        let mut result = String::with_capacity(sanitized.len());
        let mut dot_count = 0u32;
        for c in sanitized.chars() {
            if c == '.' {
                dot_count += 1;
                if dot_count <= 1 {
                    result.push(c);
                }
            } else {
                dot_count = 0;
                result.push(c);
            }
        }

        // If result is empty after sanitization, use a placeholder
        if result.is_empty() || result == "." {
            return "sanitized_file".to_string();
        }

        result
    }

    #[cfg(feature = "html-sanitizer")]
    pub fn sanitize_html(input: &str) -> String {
        ammonia::clean(input)
    }
}
