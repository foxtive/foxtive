use std::collections::HashSet;

/// Known compression and archive extensions that should be treated as single units
pub const COMPOUND_EXTENSIONS: &[&str] = &[
    "tar.gz", "tar.bz2", "tar.xz", "tar.zst", "tar.lz", "tar.lzma", "tar.lzo", "tar.sz", "tar.br",
    "tar.Z", "tbz2", "tgz", "txz", "tlz", "tzst", "tbr", "zip", "zip.gpg", "zip.aes", "7z",
    "7z.001", "7z.gpg", "gz", "bz2", "xz", "zst", "lz", "lzma", "lzo", "sz", "br",
];

pub struct FileExtHelper {
    known_exts: HashSet<String>,
}

impl FileExtHelper {
    pub fn new() -> Self {
        let mut known_exts = HashSet::new();
        for ext in COMPOUND_EXTENSIONS {
            known_exts.insert(ext.to_string());
        }
        Self { known_exts }
    }

    /// Create a new handler with default extensions plus custom ones
    pub fn with_additional_extensions<I>(extensions: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        let mut handler = Self::new();
        for ext in extensions {
            handler.add_extension(&ext);
        }
        handler
    }

    /// Add a custom extension to the handler
    pub fn add_extension(&mut self, ext: &str) {
        self.known_exts.insert(ext.to_string());
    }

    /// Extract extension from filename, handling known compression extensions
    pub fn get_extension(&self, filename: &str) -> Option<String> {
        let filename = filename.trim();

        // Handle empty or invalid filenames
        if filename.is_empty() || filename == "." || filename == ".." {
            return None;
        }

        // Check for known extensions (try longest matches first)
        let mut sorted_exts: Vec<_> = self.known_exts.iter().collect();
        sorted_exts.sort_by_key(|b| std::cmp::Reverse(b.len())); // Sort by length descending

        for ext in sorted_exts {
            if filename.to_lowercase().ends_with(&format!(".{ext}")) {
                return Some(format!("{ext}"));
            }
        }

        // Fall back to single-level extension
        if let Some(dot_pos) = filename.rfind('.') {
            // Don't treat hidden files as extensions (e.g., .gitignore)
            if dot_pos == 0 {
                return None;
            }

            // Don't treat files ending with just a dot as having an extension
            if dot_pos == filename.len() - 1 {
                return None;
            }

            let ext = &filename[(dot_pos + 1)..];   // return without dot
            Some(ext.to_string())
        } else {
            None
        }
    }

    /// Remove extension from filename, handling known compression extensions
    pub fn remove_extension(&self, filename: &str) -> String {
        let filename = filename.trim();

        // Handle empty or invalid filenames
        if filename.is_empty() || filename == "." || filename == ".." {
            return filename.to_string();
        }

        // Check for known extensions (try longest matches first)
        let mut sorted_exts: Vec<_> = self.known_exts.iter().collect();
        sorted_exts.sort_by_key(|b| std::cmp::Reverse(b.len())); // Sort by length descending

        for ext in sorted_exts {
            let full_ext = format!(".{ext}");
            if filename.to_lowercase().ends_with(&full_ext) {
                return filename[..filename.len() - full_ext.len()].to_string();
            }
        }

        // Fall back to single-level extension removal
        if let Some(dot_pos) = filename.rfind('.') {
            // Don't remove from hidden files (e.g., .gitignore)
            if dot_pos == 0 {
                return filename.to_string();
            }

            // Don't remove if file ends with just a dot
            if dot_pos == filename.len() - 1 {
                return filename.to_string();
            }

            filename[..dot_pos].to_string()
        } else {
            filename.to_string()
        }
    }

    /// Get the base name and extension as a tuple
    pub fn split_filename(&self, filename: &str) -> (String, Option<String>) {
        let base = self.remove_extension(filename);
        let ext = self.get_extension(filename);
        (base, ext)
    }
}

impl Default for FileExtHelper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_extensions() {
        let handler = FileExtHelper::new();

        // Test tar.gz variants
        assert_eq!(
            handler.get_extension("archive.tar.gz"),
            Some("tar.gz".to_string())
        );
        assert_eq!(handler.remove_extension("archive.tar.gz"), "archive");

        // Test other tar variants
        assert_eq!(
            handler.get_extension("backup.tar.bz2"),
            Some("tar.bz2".to_string())
        );
        assert_eq!(handler.remove_extension("backup.tar.bz2"), "backup");

        assert_eq!(
            handler.get_extension("data.tar.xz"),
            Some("tar.xz".to_string())
        );
        assert_eq!(handler.remove_extension("data.tar.xz"), "data");

        // Test short forms
        assert_eq!(
            handler.get_extension("archive.tgz"),
            Some("tgz".to_string())
        );
        assert_eq!(handler.remove_extension("archive.tgz"), "archive");

        assert_eq!(
            handler.get_extension("backup.tbz2"),
            Some("tbz2".to_string())
        );
        assert_eq!(handler.remove_extension("backup.tbz2"), "backup");

        // Test zip variants
        assert_eq!(handler.get_extension("file.zip"), Some("zip".to_string()));
        assert_eq!(handler.remove_extension("file.zip"), "file");

        assert_eq!(
            handler.get_extension("encrypted.zip.gpg"),
            Some("zip.gpg".to_string())
        );
        assert_eq!(handler.remove_extension("encrypted.zip.gpg"), "encrypted");

        // Test 7z variants
        assert_eq!(handler.get_extension("archive.7z"), Some("7z".to_string()));
        assert_eq!(handler.remove_extension("archive.7z"), "archive");

        assert_eq!(
            handler.get_extension("multi.7z.001"),
            Some("7z.001".to_string())
        );
        assert_eq!(handler.remove_extension("multi.7z.001"), "multi");

        // Test single compression formats
        assert_eq!(handler.get_extension("file.gz"), Some("gz".to_string()));
        assert_eq!(handler.remove_extension("file.gz"), "file");

        assert_eq!(handler.get_extension("data.bz2"), Some("bz2".to_string()));
        assert_eq!(handler.remove_extension("data.bz2"), "data");

        assert_eq!(handler.get_extension("backup.xz"), Some("xz".to_string()));
        assert_eq!(handler.remove_extension("backup.xz"), "backup");
    }

    #[test]
    fn test_single_level_extensions() {
        let handler = FileExtHelper::new();

        assert_eq!(handler.get_extension("file.txt"), Some("txt".to_string()));
        assert_eq!(handler.remove_extension("file.txt"), "file");

        assert_eq!(handler.get_extension("image.jpg"), Some("jpg".to_string()));
        assert_eq!(handler.remove_extension("image.jpg"), "image");

        assert_eq!(
            handler.get_extension("document.pdf"),
            Some("pdf".to_string())
        );
        assert_eq!(handler.remove_extension("document.pdf"), "document");
    }

    #[test]
    fn test_no_extension() {
        let handler = FileExtHelper::new();

        assert_eq!(handler.get_extension("filename"), None);
        assert_eq!(handler.remove_extension("filename"), "filename");

        assert_eq!(handler.get_extension("Makefile"), None);
        assert_eq!(handler.remove_extension("Makefile"), "Makefile");
    }

    #[test]
    fn test_hidden_files() {
        let handler = FileExtHelper::new();

        // Hidden files should not be treated as extensions
        assert_eq!(handler.get_extension("gitignore"), None);
        assert_eq!(handler.remove_extension(".gitignore"), ".gitignore");

        assert_eq!(handler.get_extension("bashrc"), None);
        assert_eq!(handler.remove_extension(".bashrc"), ".bashrc");

        // But hidden files with extensions should work
        assert_eq!(
            handler.get_extension("hidden.txt"),
            Some("txt".to_string())
        );
        assert_eq!(handler.remove_extension(".hidden.txt"), ".hidden");
    }

    #[test]
    fn test_edge_cases() {
        let handler = FileExtHelper::new();

        // Empty string
        assert_eq!(handler.get_extension(""), None);
        assert_eq!(handler.remove_extension(""), "");

        // Just dots
        assert_eq!(handler.get_extension("."), None);
        assert_eq!(handler.remove_extension("."), ".");

        assert_eq!(handler.get_extension(".."), None);
        assert_eq!(handler.remove_extension(".."), "..");

        // File ending with dot
        assert_eq!(handler.get_extension("file."), None);
        assert_eq!(handler.remove_extension("file."), "file.");

        // Multiple dots
        assert_eq!(
            handler.get_extension("file.name.txt"),
            Some("txt".to_string())
        );
        assert_eq!(handler.remove_extension("file.name.txt"), "file.name");
    }

    #[test]
    fn test_custom_extension() {
        let mut handler = FileExtHelper::new();
        handler.add_extension("custom.comp");

        assert_eq!(
            handler.get_extension("file.custom.comp"),
            Some("custom.comp".to_string())
        );
        assert_eq!(handler.remove_extension("file.custom.comp"), "file");
    }

    #[test]
    fn test_split_filename() {
        let handler = FileExtHelper::new();

        assert_eq!(
            handler.split_filename("archive.tar.gz"),
            ("archive".to_string(), Some("tar.gz".to_string()))
        );
        assert_eq!(
            handler.split_filename("file.txt"),
            ("file".to_string(), Some("txt".to_string()))
        );
        assert_eq!(handler.split_filename("noext"), ("noext".to_string(), None));
        assert_eq!(
            handler.split_filename("data.zip.gpg"),
            ("data".to_string(), Some("zip.gpg".to_string()))
        );
        assert_eq!(
            handler.split_filename("multi.7z.001"),
            ("multi".to_string(), Some("7z.001".to_string()))
        );
    }
}
