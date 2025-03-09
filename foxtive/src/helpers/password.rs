use crate::prelude::AppResult;

pub struct Password {
    salt: String,
}

impl Password {
    pub fn new(salt: String) -> Password {
        Password { salt }
    }

    pub fn hash(&self, pwd: &str) -> AppResult<String> {
        let config = argon2::Config::default();
        Ok(argon2::hash_encoded(
            pwd.as_bytes(),
            self.salt.as_bytes(),
            &config,
        )?)
    }

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
