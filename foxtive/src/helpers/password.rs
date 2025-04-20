use crate::prelude::AppResult;

/// A struct for handling password hashing and verification using Argon2.
///
/// The `Password` struct provides a secure way to hash passwords and verify them using the Argon2
/// password hashing algorithm. It maintains a salt value that is used in the hashing process.
///
/// # Examples
///
/// ```
/// use foxtive::helpers::password::Password;
///
/// // Create a new Password instance with a salt
/// let password = Password::new("unique_salt".to_string());
///
/// // Hash a password
/// let hash = password.hash("my_secret_password").unwrap();
///
/// // Verify a password against a hash
/// assert!(password.verify(&hash, "my_secret_password").unwrap());
/// assert!(!password.verify(&hash, "wrong_password").unwrap());
/// ```
pub struct Password {
    salt: String,
}

impl Password {
    /// Creates a new `Password` instance with the specified salt.
    ///
    /// The salt should be a unique, random string that will be used in the password hashing process.
    /// It's recommended to use a cryptographically secure random generator to create the salt.
    ///
    /// # Arguments
    ///
    /// * `salt` - A string that will be used as the salt in the password hashing process
    ///
    /// # Examples
    ///
    /// ```
    /// use foxtive::helpers::password::Password;
    ///
    /// let password = Password::new("unique_salt".to_string());
    /// ```
    pub fn new(salt: String) -> Password {
        Password { salt }
    }

    /// Hashes a password string using Argon2 with the instance's salt.
    ///
    /// This method uses the default Argon2 configuration parameters and combines the provided
    /// password with the instance's salt to create a secure hash.
    ///
    /// # Arguments
    ///
    /// * `pwd` - The password string to hash
    ///
    /// # Returns
    ///
    /// * `AppResult<String>` - A Result containing either the encoded hash string or an error
    ///
    /// # Errors
    ///
    /// Returns an error if the Argon2 hashing process fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use foxtive::helpers::password::Password;
    ///
    /// let password = Password::new("unique_salt".to_string());
    /// let hash = password.hash("my_secret_password").unwrap();
    /// ```
    pub fn hash(&self, pwd: &str) -> AppResult<String> {
        let config = argon2::Config::default();
        Ok(argon2::hash_encoded(
            pwd.as_bytes(),
            self.salt.as_bytes(),
            &config,
        )?)
    }

    /// Verifies a password against a previously generated hash.
    ///
    /// This method checks if the provided password matches the provided hash. The hash should
    /// have been generated using the same salt that the Password instance was created with.
    ///
    /// # Arguments
    ///
    /// * `hash` - The encoded hash string to verify against
    /// * `password` - The password to verify
    ///
    /// # Returns
    ///
    /// * `AppResult<bool>` - A Result containing either:
    ///   * `true` if the password matches the hash
    ///   * `false` if the password doesn't match the hash
    ///   * An error if the verification process fails
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The hash string is invalid or malformed
    /// * The Argon2 verification process fails
    ///
    /// # Examples
    ///
    /// ```
    /// use foxtive::helpers::password::Password;
    ///
    /// let password = Password::new("unique_salt".to_string());
    /// let hash = password.hash("my_secret_password").unwrap();
    ///
    /// // Verify the correct password
    /// assert!(password.verify(&hash, "my_secret_password").unwrap());
    ///
    /// // Verify incorrect password
    /// assert!(!password.verify(&hash, "wrong_password").unwrap());
    /// ```
    pub fn verify(&self, hash: &str, password: &str) -> AppResult<bool> {
        Ok(argon2::verify_encoded(hash, password.as_bytes())?)
    }
}

#[cfg(test)]
mod tests {
    use argon2::{self, Error};

    use super::*;

    #[test]
    fn test_password_new() {
        let salt = "random_salt".to_string();
        let password = Password::new(salt.clone());
        assert_eq!(password.salt, salt);
    }

    #[test]
    fn test_password_hash() {
        let salt = "random_salt".to_string();
        let password = Password::new(salt.clone());
        let pwd = "my_password";

        let hash = password.hash(pwd).unwrap();

        // Verify the generated hash is not empty
        assert!(!hash.is_empty());

        // Verify that the hash can be successfully verified
        assert!(argon2::verify_encoded(&hash, pwd.as_bytes()).unwrap());
    }

    #[test]
    fn test_password_verify_correct() {
        let salt = "random_salt".to_string();
        let password = Password::new(salt.clone());
        let pwd = "my_password";

        let hash = password.hash(pwd).unwrap();

        // Verify that the password matches the hash
        assert!(password.verify(&hash, pwd).unwrap());
    }

    #[test]
    fn test_password_verify_incorrect() {
        let salt = "random_salt".to_string();
        let password = Password::new(salt.clone());
        let pwd = "my_password";

        let hash = password.hash(pwd).unwrap();

        // Try to verify with a different password
        let incorrect_password = "wrong_password";
        assert!(!password.verify(&hash, incorrect_password).unwrap())
    }

    #[test]
    fn test_password_verify_invalid_hash() {
        let salt = "random_salt".to_string();
        let password = Password::new(salt.clone());
        let invalid_hash = "invalid_hash";
        let pwd = "my_password";

        // Verify that using an invalid hash returns error
        let err = password
            .verify(invalid_hash, pwd)
            .unwrap_err()
            .downcast::<Error>()
            .unwrap();

        assert_eq!(err, Error::DecodingFail);
    }
}
