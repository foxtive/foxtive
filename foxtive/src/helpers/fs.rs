use std::env;
use std::path::{Path, PathBuf};

/// Returns the current working directory as a String.
pub fn get_cwd() -> String {
    env::current_dir().unwrap().to_str().unwrap().to_string()
}

/// Returns the current working directory as a PathBuf.
pub fn get_cwd_buff() -> PathBuf {
    env::current_dir().unwrap()
}

/// Constructs an absolute path relative to the current working directory.
pub fn base_path<P: AsRef<Path>>(path: P) -> PathBuf {
    let mut loc = get_cwd_buff();
    loc.push(path);
    loc
}
