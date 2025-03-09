use std::env;
use std::path::{Path, PathBuf};

pub fn get_cwd() -> String {
    env::current_dir().unwrap().to_str().unwrap().to_string()
}

pub fn get_cwd_buff() -> PathBuf {
    env::current_dir().unwrap()
}

pub fn base_path<P: AsRef<Path>>(path: P) -> PathBuf {
    let mut loc = get_cwd_buff();
    loc.push(path);
    loc
}
