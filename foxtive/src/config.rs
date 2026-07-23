//! Type-safe configuration management.
//!
//! Provides utilities for loading typed configuration from environment
//! variables and files.
//!
//! # Example
//!
//! ```no_run
//! use foxtive::config;
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct MyAppConfig {
//!     database_url: String,
//!     max_connections: u32,
//! }
//!
//! # fn main() -> foxtive::results::AppResult<()> {
//! let config = config::from_env::<MyAppConfig>("MYAPP")?;
//! # Ok(())
//! # }
//! ```

use crate::enums::AppMessage;
use crate::prelude::AppResult;
use serde::de::DeserializeOwned;

/// Load a configuration struct from a JSON environment variable.
///
/// The environment variable is expected to contain a JSON object.
/// Variable name: `{PREFIX}_CONFIG`
///
/// # Errors
/// Returns an error if the variable is not set or contains invalid JSON.
pub fn from_env<T: DeserializeOwned>(prefix: &str) -> AppResult<T> {
    let var_name = format!("{prefix}_CONFIG");
    let json = std::env::var(&var_name)
        .map_err(|e| AppMessage::missing_environment_variable(&var_name, e))?;
    serde_json::from_str(&json).map_err(|e| AppMessage::Infrastructure {
        message: format!("Failed to parse config from env var '{var_name}': {e}"),
        source: Some(Box::new(e)),
    })
}

/// Load a configuration struct from a JSON file.
///
/// # Errors
/// Returns an error if the path is empty, the file does not exist,
/// is not a regular file, or contains invalid JSON.
pub fn from_file<T: DeserializeOwned>(path: &str) -> AppResult<T> {
    if path.is_empty() {
        return Err(AppMessage::Infrastructure {
            message: "Config file path is empty".into(),
            source: None,
        });
    }

    let path = std::path::Path::new(path);
    if !path.exists() {
        return Err(AppMessage::Infrastructure {
            message: format!("Config file not found: {}", path.display()),
            source: None,
        });
    }
    if !path.is_file() {
        return Err(AppMessage::Infrastructure {
            message: format!("Config path is not a regular file: {}", path.display()),
            source: None,
        });
    }

    let content = std::fs::read_to_string(path)?;
    serde_json::from_str(&content).map_err(|e| AppMessage::Infrastructure {
        message: format!("Failed to parse config from file '{}': {e}", path.display()),
        source: Some(Box::new(e)),
    })
}

/// Load a configuration struct, trying file first then env vars as fallback.
///
/// Looks for `{service}/config.json` first, then falls back to the
/// `{PREFIX}_CONFIG` environment variable.
///
/// # Path Resolution
///
/// The file path is resolved relative to the **executable directory** (the directory
/// containing the running binary). If the executable directory cannot be determined,
/// falls back to the current working directory.
pub fn load<T: DeserializeOwned>(service: &str, prefix: &str) -> AppResult<T> {
    // Try to resolve path relative to executable directory first
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let cwd = std::env::current_dir().ok();

    // Try executable directory first, then CWD
    let file_path = exe_dir
        .as_ref()
        .map(|dir| dir.join(service).join("config.json"))
        .or_else(|| cwd.as_ref().map(|dir| dir.join(service).join("config.json")));

    if let Some(path) = file_path
        && path.exists()
    {
        return from_file(path.to_str().unwrap_or(""));
    }

    from_env(prefix)
}
