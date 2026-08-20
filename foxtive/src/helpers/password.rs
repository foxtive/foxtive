use crate::prelude::AppResult;
use zeroize::Zeroizing;

/// Result of a password verification attempt.
///
/// Returned by [`Password::verify_ex()`] to indicate which format matched,
/// allowing the caller to decide whether to rehash the password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyResult {
    /// Matched the current pepper format - hash is up to date.
    Matched,
    /// Matched the legacy (pre-pepper) format - password is correct but the
    /// hash should be upgraded via [`Password::hash()`] and persisted.
    MatchedLegacy,
    /// No match - password is incorrect.
    Mismatch,
}

impl VerifyResult {
    /// Returns `true` if the password matched (either format).
    pub fn is_match(&self) -> bool {
        matches!(self, VerifyResult::Matched | VerifyResult::MatchedLegacy)
    }

    /// Returns `true` if the hash should be upgraded to the current format.
    pub fn needs_rehash(&self) -> bool {
        matches!(self, VerifyResult::MatchedLegacy)
    }
}

/// A struct for handling password hashing and verification using Argon2.
///
/// The `Password` struct provides a secure way to hash passwords and verify them using the Argon2
/// password hashing algorithm. Each hash uses a unique random salt that is embedded in the
/// output hash string, so no separate salt tracking is needed.
///
/// The `pepper` field serves as a **pepper** - a server-wide secret appended to all
/// passwords for defense-in-depth. Unlike per-hash salts, the pepper is stored separately
/// from the database (e.g., in environment variables). The pepper is zeroized on drop.
///
/// # Production Configuration
///
/// The default Argon2 parameters may be too light for intense production workloads.
/// For production use, configure explicit parameters via [`Password::with_config()`]:
///
/// ```no_run
/// use foxtive::helpers::password::Password;
///
/// let config = argon2::Config {
///     mem_cost: 65536,  // 64 MB - increases memory-hardness
///     time_cost: 3,     // 3 iterations - increases CPU cost
///     lanes: 4,         // 4 threads - increases parallel cost
///     ..Default::default()
/// };
/// let password = Password::with_config("server_pepper".to_string(), config);
/// ```
///
/// **Recommended production parameters:**
/// - `mem_cost`: 65536 (64 MB) or higher for sensitive accounts
/// - `time_cost`: 3 or higher
/// - `lanes`: 4 or higher
///
/// # Legacy Hash Compatibility
///
/// Passwords hashed with the legacy format (raw password bytes, no pepper) are
/// automatically detected during verification. Use [`verify_ex()`](Self::verify_ex)
/// to get a [`VerifyResult`] that indicates whether the hash needs upgrading.
///
/// # Examples
///
/// ```
/// use foxtive::helpers::password::{Password, VerifyResult};
///
/// let password = Password::new("server_pepper".to_string());
///
/// // Hash a password (random salt is generated per-hash)
/// let hash = password.hash("my_secret_password").unwrap();
///
/// // Verify - returns detailed result
/// let result = password.verify_ex(&hash, "my_secret_password").unwrap();
/// assert_eq!(result, VerifyResult::Matched);
/// assert!(!result.needs_rehash());
///
/// // Simple boolean verify also works
/// assert!(password.verify(&hash, "my_secret_password").unwrap());
/// ```
#[derive(Clone)]
pub struct Password {
    pepper: Zeroizing<String>,
    /// Optional custom Argon2 configuration for tuning hash parameters
    config: Option<argon2::Config<'static>>,
}

impl Password {
    /// Creates a new `Password` instance with the specified pepper.
    ///
    /// The pepper is a server-wide secret used as additional entropy alongside
    /// the per-hash random salt. Pass an empty string if no pepper is desired.
    ///
    /// # Arguments
    ///
    /// * `pepper` - A server-wide secret appended to all passwords for defense-in-depth
    pub fn new(pepper: String) -> Password {
        Password {
            pepper: Zeroizing::new(pepper),
            config: None,
        }
    }

    /// Creates a new `Password` instance with custom Argon2 configuration.
    ///
    /// This allows tuning Argon2 parameters (memory cost, time cost, lanes/parallelism)
    /// for production requirements. Higher values increase security but also CPU/memory usage.
    ///
    /// # Arguments
    ///
    /// * `pepper` - A server-wide secret appended to all passwords for defense-in-depth
    /// * `config` - Custom Argon2 configuration
    ///
    /// # Examples
    ///
    /// ```
    /// use foxtive::helpers::password::Password;
    ///
    /// let mut config = argon2::Config::default();
    /// config.mem_cost = 65536; // 64 MB
    /// config.time_cost = 3;
    /// let password = Password::with_config("pepper".to_string(), config);
    /// ```
    pub fn with_config(pepper: String, config: argon2::Config<'static>) -> Password {
        Password {
            pepper: Zeroizing::new(pepper),
            config: Some(config),
        }
    }

    /// Hashes a password string using Argon2 with a per-hash random salt.
    ///
    /// A unique random salt is generated for each call. The salt is embedded in
    /// the output hash string and extracted automatically during verification.
    /// No separate salt storage is needed.
    ///
    /// The password is combined with the pepper using a null-byte separator
    /// (`password\x00pepper`) before hashing.
    ///
    /// # Arguments
    ///
    /// * `pwd` - The password string to hash
    ///
    /// # Returns
    ///
    /// * `AppResult<String>` - A Result containing the encoded hash string
    pub fn hash(&self, pwd: &str) -> AppResult<String> {
        let config = self.config.as_ref().cloned().unwrap_or_default();
        // Combine password with pepper using null-byte separator to prevent ambiguity
        let peppered = format!("{pwd}\x00{}", *self.pepper);
        // Generate a unique random 16-byte salt per hash
        let mut salt_bytes = [0u8; 16];
        use rand::RngExt;
        rand::rng().fill(&mut salt_bytes);
        Ok(argon2::hash_encoded(
            peppered.as_bytes(),
            &salt_bytes,
            &config,
        )?)
    }

    /// Verifies a password against a previously generated hash.
    ///
    /// The salt is extracted from the encoded hash string automatically.
    /// Tries the current pepper format first, then falls back to the legacy
    /// format (raw password bytes) for backward compatibility.
    ///
    /// For detailed results (including whether the hash needs upgrading),
    /// use [`verify_ex()`](Self::verify_ex) instead.
    ///
    /// # Arguments
    ///
    /// * `hash` - The encoded hash string to verify against
    /// * `password` - The password to verify
    ///
    /// # Returns
    ///
    /// * `AppResult<bool>` - `true` if the password matches (either format), `false` otherwise
    pub fn verify(&self, hash: &str, password: &str) -> AppResult<bool> {
        Ok(self.verify_ex(hash, password)?.is_match())
    }

    /// Verifies a password and returns a detailed [`VerifyResult`].
    ///
    /// This is the extended verification method that distinguishes between
    /// current-format and legacy-format matches, enabling transparent hash
    /// upgrades on login.
    ///
    /// # Verification Order
    ///
    /// 1. Try current format: `password\x00pepper`
    /// 2. Fall back to legacy format: raw `password` bytes
    ///
    /// # Upgrading Legacy Hashes
    ///
    /// When `verify_ex()` returns [`VerifyResult::MatchedLegacy`], the password
    /// is correct but the stored hash was created without a pepper. The caller
    /// should rehash the password and persist the new hash:
    ///
    /// ```ignore
    /// let result = password.verify_ex(&stored_hash, &user_password)?;
    /// match result {
    ///     VerifyResult::Matched => { /* all good */ }
    ///     VerifyResult::MatchedLegacy => {
    ///         let new_hash = password.hash(&user_password)?;
    ///         user_repo.update_hash(user_id, &new_hash).await?;
    ///     }
    ///     VerifyResult::Mismatch => { /* reject login */ }
    /// }
    /// ```
    ///
    /// # Arguments
    ///
    /// * `hash` - The encoded hash string to verify against
    /// * `password` - The password to verify
    ///
    /// # Returns
    ///
    /// * `AppResult<VerifyResult>` - The verification outcome
    pub fn verify_ex(&self, hash: &str, password: &str) -> AppResult<VerifyResult> {
        // Try current format: password\x00pepper
        let peppered = format!("{password}\x00{}", *self.pepper);
        match argon2::verify_encoded(hash, peppered.as_bytes()) {
            Ok(true) => return Ok(VerifyResult::Matched),
            Ok(false) => {} // no match, try legacy
            Err(e) => {
                // Hash is malformed or uses unsupported parameters.
                // The legacy format would hit the same parsing error,
                // so propagate immediately.
                return Err(e.into());
            }
        }

        // Fall back to legacy format: raw password bytes (pre-pepper era)
        match argon2::verify_encoded(hash, password.as_bytes()) {
            Ok(true) => Ok(VerifyResult::MatchedLegacy),
            Ok(false) => Ok(VerifyResult::Mismatch),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a legacy hash (raw password bytes, no pepper transformation)
    /// simulating hashes produced by the old Password implementation.
    fn legacy_hash(password: &str) -> String {
        let config = argon2::Config::default();
        let mut salt_bytes = [0u8; 16];
        use rand::RngExt;
        rand::rng().fill(&mut salt_bytes);
        argon2::hash_encoded(password.as_bytes(), &salt_bytes, &config).unwrap()
    }

    #[test]
    fn test_password_new() {
        let pepper = "random_pepper".to_string();
        let password = Password::new(pepper.clone());
        assert_eq!(&*password.pepper, &pepper);
    }

    #[test]
    fn test_password_hash() {
        let pepper = "random_pepper".to_string();
        let password = Password::new(pepper.clone());
        let pwd = "my_password";

        let hash = password.hash(pwd).unwrap();

        assert!(!hash.is_empty());
        assert!(password.verify(&hash, pwd).unwrap());
    }

    #[test]
    fn test_password_verify_correct() {
        let pepper = "random_pepper".to_string();
        let password = Password::new(pepper.clone());
        let pwd = "my_password";

        let hash = password.hash(pwd).unwrap();
        assert!(password.verify(&hash, pwd).unwrap());
    }

    #[test]
    fn test_password_verify_incorrect() {
        let pepper = "random_pepper".to_string();
        let password = Password::new(pepper.clone());
        let pwd = "my_password";

        let hash = password.hash(pwd).unwrap();

        let incorrect_password = "wrong_password";
        assert!(!password.verify(&hash, incorrect_password).unwrap())
    }

    #[test]
    fn test_password_verify_invalid_hash() {
        let pepper = "random_pepper".to_string();
        let password = Password::new(pepper.clone());
        let invalid_hash = "invalid_hash";
        let pwd = "my_password";

        let err = password.verify(invalid_hash, pwd).unwrap_err();

        assert!(err.is_server_error());
    }

    #[test]
    fn test_per_hash_random_salt() {
        let password = Password::new("pepper".to_string());
        let pwd = "same_password";

        let hash1 = password.hash(pwd).unwrap();
        let hash2 = password.hash(pwd).unwrap();
        assert_ne!(hash1, hash2, "Each hash should use a unique random salt");

        assert!(password.verify(&hash1, pwd).unwrap());
        assert!(password.verify(&hash2, pwd).unwrap());
    }

    #[test]
    fn test_different_pepper_fails_verification() {
        let password1 = Password::new("pepper_a".to_string());
        let password2 = Password::new("pepper_b".to_string());
        let pwd = "my_password";

        let hash = password1.hash(pwd).unwrap();

        // Different pepper: new format fails, legacy also fails (hash wasn't raw)
        assert!(!password2.verify(&hash, pwd).unwrap());
    }

    #[test]
    fn test_verify_ex_legacy_hash_matches() {
        // Simulate a hash created by the old code (raw password, no pepper)
        let legacy = legacy_hash("my_password");

        // New Password with a pepper should still verify legacy hashes
        let password = Password::new("some_pepper".to_string());
        let result = password.verify_ex(&legacy, "my_password").unwrap();

        assert_eq!(result, VerifyResult::MatchedLegacy);
        assert!(result.is_match());
        assert!(result.needs_rehash());
    }

    #[test]
    fn test_verify_ex_new_hash_matches_current() {
        let password = Password::new("my_pepper".to_string());
        let hash = password.hash("my_password").unwrap();

        let result = password.verify_ex(&hash, "my_password").unwrap();

        assert_eq!(result, VerifyResult::Matched);
        assert!(result.is_match());
        assert!(!result.needs_rehash());
    }

    #[test]
    fn test_verify_ex_wrong_password_returns_mismatch() {
        let password = Password::new("my_pepper".to_string());
        let hash = password.hash("my_password").unwrap();

        let result = password.verify_ex(&hash, "wrong_password").unwrap();

        assert_eq!(result, VerifyResult::Mismatch);
        assert!(!result.is_match());
    }

    #[test]
    fn test_verify_ex_legacy_wrong_password() {
        let legacy = legacy_hash("my_password");
        let password = Password::new("some_pepper".to_string());

        let result = password.verify_ex(&legacy, "wrong_password").unwrap();

        assert_eq!(result, VerifyResult::Mismatch);
    }

    #[test]
    fn test_verify_boolean_works_for_both_formats() {
        let password = Password::new("my_pepper".to_string());

        // New format hash
        let new_hash = password.hash("password123").unwrap();
        assert!(password.verify(&new_hash, "password123").unwrap());

        // Legacy format hash
        let old_hash = legacy_hash("password123");
        assert!(password.verify(&old_hash, "password123").unwrap());
    }

    #[test]
    fn test_verify_ex_empty_pepper_legacy() {
        // Edge case: Password created with empty pepper
        // Legacy hashes should still match via the legacy fallback
        let legacy = legacy_hash("my_password");
        let password = Password::new(String::new());

        // The legacy hash was created with raw bytes.
        // With empty pepper, new format is "password\x00" which won't match.
        // Legacy fallback with raw bytes will match.
        let result = password.verify_ex(&legacy, "my_password").unwrap();
        assert_eq!(result, VerifyResult::MatchedLegacy);
    }

    #[test]
    fn test_rehash_upgrades_legacy_hash() {
        let password = Password::new("production_pepper".to_string());

        // Start with a legacy hash
        let legacy = legacy_hash("my_password");
        let result = password.verify_ex(&legacy, "my_password").unwrap();
        assert!(result.needs_rehash());

        // Rehash with current format
        let new_hash = password.hash("my_password").unwrap();

        // The new hash should verify as current format
        let result2 = password.verify_ex(&new_hash, "my_password").unwrap();
        assert_eq!(result2, VerifyResult::Matched);
        assert!(!result2.needs_rehash());
    }
}
