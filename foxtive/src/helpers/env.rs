//! Environment variable reading with optional prefixes, plus `.env` file loading.
//!
//! Two layers are provided:
//! - Free functions [`var`] / [`var_or`] taking an explicit `env_prefix` (back-compat API).
//! - [`ScopedEnv`], a prefix-carrying value type that delegates to the same helpers.
//!   Obtain one via [`App::env_vars()`](crate::App::env_vars),
//!   [`AppBuilder::env_vars()`](crate::AppBuilder::env_vars), [`App::load_env_files`](crate::App::load_env_files)
//!   or [`ScopedEnv::new`].
//!
//! An empty prefix means bare key lookup (no leading underscore).
//!
//! # Example
//!
//! ```
//! use foxtive::ScopedEnv;
//!
//! let env = ScopedEnv::new("MYAPP");
//! assert_eq!(env.key("DB_HOST"), "MYAPP_DB_HOST");
//!
//! let bare = ScopedEnv::unprefixed();
//! assert_eq!(bare.key("DB_HOST"), "DB_HOST");
//! ```

use crate::enums::AppMessage;
use crate::results::AppResult;
use std::env::{self, VarError};
use std::path::Path;
use std::str::FromStr;

/// Builds the effective environment variable name for a prefix and key.
///
/// Returns the bare `key` when `env_prefix` is empty, otherwise `"{prefix}_{key}"`.
pub fn prefixed_key(env_prefix: &str, key: &str) -> String {
    if env_prefix.is_empty() {
        key.to_string()
    } else {
        format!("{env_prefix}_{key}")
    }
}

/// Reads a required environment variable using the given prefix.
///
/// # Errors
/// Returns [`AppMessage::MissingEnvironmentVariable`] when the variable is not set.
pub fn var(env_prefix: &str, key: &str) -> AppResult<String> {
    let key = prefixed_key(env_prefix, key);
    env::var(&key).map_err(|e| AppMessage::MissingEnvironmentVariable(key, e))
}

/// Reads an environment variable using the given prefix, falling back to `default`.
pub fn var_or(env_prefix: &str, key: &str, default: String) -> String {
    let key = prefixed_key(env_prefix, key);
    env::var(&key).unwrap_or(default)
}

/// Loads `.env` files in order (fail-fast).
///
/// Each path must exist and be readable - a missing or malformed file returns an
/// error naming the path. `dotenvy` never overwrites already-set variables, so the
/// *first* file that sets a key wins - pass the most-specific file first
/// (e.g. `apps/{svc}/.env`, then root `.env`).
///
/// This is the shared loader behind [`App::load_env_files`] and the builder's
/// `.env_file()` / `.env_files()` methods.
pub(crate) fn load_files<P: AsRef<Path>>(paths: impl IntoIterator<Item = P>) -> AppResult<()> {
    for path in paths {
        let path = path.as_ref();
        dotenvy::from_filename(path).map_err(|e| AppMessage::Infrastructure {
            message: format!("Failed to load env file '{}': {e}", path.display()),
            source: None,
        })?;
    }
    Ok(())
}

/// A prefix-carrying environment variable reader.
///
/// `ScopedEnv` bundles an env-var prefix (e.g. `"QUARKAXIS"`) so it can be set up
/// once and passed around, replacing manual `env::var(prefix, key)` threading.
/// It is cheap to clone (a single small `String`).
///
/// Note: `ScopedEnv` (the reader) is distinct from [`Environment`](crate::Environment)
/// (the `Local`/`Dev`/`Prod` enum). [`App::env()`](crate::App::env) keeps returning
/// `Environment`.
///
/// # Example
///
/// ```
/// use foxtive::ScopedEnv;
///
/// // Replaces `fn my_service(env_prefix: &str)` threading:
/// fn read_timeout(env: &ScopedEnv) -> u64 {
///     env.parse_or("TIMEOUT_SECONDS", 30)
/// }
///
/// let env = ScopedEnv::new("MYAPP");
/// assert_eq!(read_timeout(&env), 30);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopedEnv {
    prefix: String,
}

impl ScopedEnv {
    /// Creates a `ScopedEnv` reading keys as `{prefix}_{key}`.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }

    /// Creates a `ScopedEnv` with an empty prefix - keys are read bare.
    pub fn unprefixed() -> Self {
        Self::new(String::new())
    }

    /// Loads `.env` files in order (fail-fast), then returns a prefix-bound `ScopedEnv`.
    ///
    /// Same behavior as [`App::load_env_files`](crate::App::load_env_files), for use without an `App`.
    ///
    /// # Errors
    /// Returns an error naming the path if any file is missing or unreadable.
    pub fn load<P: AsRef<Path>>(
        prefix: impl Into<String>,
        paths: impl IntoIterator<Item = P>,
    ) -> AppResult<Self> {
        load_files(paths)?;
        Ok(Self::new(prefix))
    }

    /// Returns the prefix this reader is bound to.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Returns the effective environment variable name for `key`.
    pub fn key(&self, key: &str) -> String {
        prefixed_key(&self.prefix, key)
    }

    /// Reads a required variable.
    ///
    /// # Errors
    /// [`AppMessage::MissingEnvironmentVariable`] when absent.
    pub fn var(&self, key: &str) -> AppResult<String> {
        var(&self.prefix, key)
    }

    /// Reads a required variable and parses it.
    ///
    /// # Errors
    /// [`AppMessage::MissingEnvironmentVariable`] when absent;
    /// [`AppMessage::InternalServerError`] when present but unparseable.
    pub fn parse<T: FromStr>(&self, key: &str) -> AppResult<T> {
        let full_key = self.key(key);
        let value = var(&self.prefix, key)?;
        value
            .parse::<T>()
            .map_err(|_| AppMessage::InternalServerError(format!("Invalid value for '{full_key}'")))
    }

    /// Reads an optional variable - `None` when absent.
    pub fn var_opt(&self, key: &str) -> Option<String> {
        env::var(self.key(key)).ok()
    }

    /// Reads an optional variable and parses it - `None` when absent *or* unparseable.
    pub fn parse_opt<T: FromStr>(&self, key: &str) -> Option<T> {
        self.var_opt(key).and_then(|v| v.parse::<T>().ok())
    }

    /// Reads a variable with a default fallback when absent.
    pub fn var_or(&self, key: &str, default: impl Into<String>) -> String {
        var_or(&self.prefix, key, default.into())
    }

    /// Reads a variable and parses it, falling back to `default` when absent *or* unparseable.
    pub fn parse_or<T: FromStr>(&self, key: &str, default: T) -> T {
        self.var_opt(key)
            .and_then(|v| v.parse::<T>().ok())
            .unwrap_or(default)
    }

    /// Reads and parses a variable, distinguishing absent from invalid.
    ///
    /// # Errors
    /// [`AppMessage::InternalServerError`] when present but unparseable.
    /// Absent is `Ok(None)`, not an error.
    pub fn try_parse<T: FromStr>(&self, key: &str) -> AppResult<Option<T>> {
        let full_key = self.key(key);
        match env::var(&full_key) {
            Err(VarError::NotPresent) => Ok(None),
            Err(e) => Err(AppMessage::MissingEnvironmentVariable(full_key, e)),
            Ok(value) => value.parse::<T>().map(Some).map_err(|_| {
                AppMessage::InternalServerError(format!("Invalid value for '{full_key}'"))
            }),
        }
    }

    /// Returns `true` when the variable is set.
    pub fn contains(&self, key: &str) -> bool {
        env::var(self.key(key)).is_ok()
    }

    /// Validates that all given keys are set.
    ///
    /// # Errors
    /// A single consolidated [`AppMessage::Infrastructure`] listing *every* missing
    /// key, so operators see all gaps at once instead of one at a time.
    pub fn require_all<K: AsRef<str>>(&self, keys: impl IntoIterator<Item = K>) -> AppResult<()> {
        let missing: Vec<String> = keys
            .into_iter()
            .map(|k| self.key(k.as_ref()))
            .filter(|full_key| env::var(full_key).is_err())
            .collect();

        if missing.is_empty() {
            return Ok(());
        }

        Err(AppMessage::Infrastructure {
            message: format!("Missing required environment variables: {}", missing.join(", ")),
            source: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Edition 2024: `set_var`/`remove_var` are unsafe and env is process-global,
    /// so all env-mutating tests serialize on this lock.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn set(key: &str, value: &str) {
        unsafe { env::set_var(key, value) };
    }

    fn remove(key: &str) {
        unsafe { env::remove_var(key) };
    }

    #[test]
    fn test_prefixed_key() {
        assert_eq!(prefixed_key("QUARKAXIS", "DB_HOST"), "QUARKAXIS_DB_HOST");
        assert_eq!(prefixed_key("", "DB_HOST"), "DB_HOST");
    }

    #[test]
    fn test_free_var_back_compat() {
        let _guard = ENV_LOCK.lock().unwrap();
        set("FX_T_VAR_A", "value-a");
        assert_eq!(var("FX_T_VAR", "A").unwrap(), "value-a");
        assert!(matches!(
            var("FX_T_VAR", "MISSING").unwrap_err(),
            AppMessage::MissingEnvironmentVariable(name, _) if name == "FX_T_VAR_MISSING"
        ));
        remove("FX_T_VAR_A");
    }

    #[test]
    fn test_free_var_empty_prefix_no_leading_underscore() {
        let _guard = ENV_LOCK.lock().unwrap();
        set("FX_T_BARE", "bare-value");
        // Regression: empty prefix used to produce "_FX_T_BARE".
        assert_eq!(var("", "FX_T_BARE").unwrap(), "bare-value");
        assert_eq!(var_or("", "FX_T_BARE", "fallback".into()), "bare-value");
        assert_eq!(var_or("", "FX_T_ABSENT", "fallback".into()), "fallback");
        remove("FX_T_BARE");
    }

    #[test]
    fn test_scoped_env_key_and_prefix() {
        let env = ScopedEnv::new("FX_T_SCOPED");
        assert_eq!(env.prefix(), "FX_T_SCOPED");
        assert_eq!(env.key("PORT"), "FX_T_SCOPED_PORT");

        let bare = ScopedEnv::unprefixed();
        assert_eq!(bare.prefix(), "");
        assert_eq!(bare.key("PORT"), "PORT");
        assert_eq!(ScopedEnv::default(), bare);
    }

    #[test]
    fn test_scoped_env_var_family() {
        let _guard = ENV_LOCK.lock().unwrap();
        let env = ScopedEnv::new("FX_T_FAM");
        set("FX_T_FAM_PRESENT", "hello");

        assert_eq!(env.var("PRESENT").unwrap(), "hello");
        assert_eq!(env.var_opt("PRESENT").as_deref(), Some("hello"));
        assert_eq!(env.var_or("PRESENT", "fallback".to_string()), "hello");
        assert_eq!(env.var_or("ABSENT", "fallback".to_string()), "fallback");
        assert!(env.contains("PRESENT"));
        assert!(!env.contains("ABSENT"));

        assert!(matches!(
            env.var("ABSENT").unwrap_err(),
            AppMessage::MissingEnvironmentVariable(name, _) if name == "FX_T_FAM_ABSENT"
        ));
        assert_eq!(env.var_opt("ABSENT"), None);

        remove("FX_T_FAM_PRESENT");
    }

    #[test]
    fn test_scoped_env_parse_family() {
        let _guard = ENV_LOCK.lock().unwrap();
        let env = ScopedEnv::new("FX_T_PARSE");
        set("FX_T_PARSE_VALID", "8080");
        set("FX_T_PARSE_INVALID", "not-a-number");

        assert_eq!(env.parse::<u16>("VALID").unwrap(), 8080);
        assert_eq!(env.parse_opt::<u16>("VALID"), Some(8080));
        assert_eq!(env.parse_or("VALID", 1u16), 8080);
        assert_eq!(env.try_parse::<u16>("VALID").unwrap(), Some(8080));

        // Absent
        assert!(matches!(
            env.parse::<u16>("ABSENT").unwrap_err(),
            AppMessage::MissingEnvironmentVariable(_, _)
        ));
        assert_eq!(env.parse_opt::<u16>("ABSENT"), None);
        assert_eq!(env.parse_or("ABSENT", 3000u16), 3000);
        assert_eq!(env.try_parse::<u16>("ABSENT").unwrap(), None);

        // Present but invalid
        assert!(matches!(
            env.parse::<u16>("INVALID").unwrap_err(),
            AppMessage::InternalServerError(_)
        ));
        assert_eq!(env.parse_opt::<u16>("INVALID"), None);
        assert_eq!(env.parse_or("INVALID", 3000u16), 3000);
        assert!(matches!(
            env.try_parse::<u16>("INVALID").unwrap_err(),
            AppMessage::InternalServerError(_)
        ));

        remove("FX_T_PARSE_VALID");
        remove("FX_T_PARSE_INVALID");
    }

    #[test]
    fn test_scoped_env_unprefixed_reads() {
        let _guard = ENV_LOCK.lock().unwrap();
        let env = ScopedEnv::unprefixed();
        set("FX_T_UNPREFIXED", "42");

        assert_eq!(env.var("FX_T_UNPREFIXED").unwrap(), "42");
        assert_eq!(env.parse::<u32>("FX_T_UNPREFIXED").unwrap(), 42);

        remove("FX_T_UNPREFIXED");
    }

    #[test]
    fn test_require_all_consolidated_error() {
        let _guard = ENV_LOCK.lock().unwrap();
        let env = ScopedEnv::new("FX_T_REQ");
        set("FX_T_REQ_ONE", "1");

        // All present -> Ok
        assert!(env.require_all(["ONE"]).is_ok());
        // Empty list -> Ok
        assert!(env.require_all(Vec::<String>::new()).is_ok());

        // Multiple missing -> one error listing every full key
        let err = env.require_all(["ONE", "TWO", "THREE"]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("FX_T_REQ_TWO"), "message was: {msg}");
        assert!(msg.contains("FX_T_REQ_THREE"), "message was: {msg}");
        assert!(!msg.contains("FX_T_REQ_ONE,"), "message was: {msg}");
        assert!(matches!(err, AppMessage::Infrastructure { .. }));

        remove("FX_T_REQ_ONE");
    }

    #[test]
    fn test_load_files_missing_is_error() {
        let err = load_files(["/definitely/not/here/.env.fx-t"]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("/definitely/not/here/.env.fx-t"), "message was: {msg}");
    }

    #[test]
    fn test_scoped_env_load_layering() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("base.env");
        let overlay = dir.path().join("overlay.env");
        std::fs::write(
            &base,
            "FX_T_LOAD_A=from-base\nFX_T_LOAD_B=from-base\n",
        )
        .unwrap();
        // dotenvy does not overwrite already-set vars, so a value set before
        // loading wins - emulate layering by loading base first.
        std::fs::write(&overlay, "FX_T_LOAD_C=from-overlay\n").unwrap();

        let env = ScopedEnv::load("FX_T_LOAD", [&base, &overlay]).unwrap();
        assert_eq!(env.prefix(), "FX_T_LOAD");
        assert_eq!(env.var("A").unwrap(), "from-base");
        assert_eq!(env.var("B").unwrap(), "from-base");
        assert_eq!(env.var("C").unwrap(), "from-overlay");

        remove("FX_T_LOAD_A");
        remove("FX_T_LOAD_B");
        remove("FX_T_LOAD_C");
    }
}
