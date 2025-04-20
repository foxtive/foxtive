//! HMAC (Hash-based Message Authentication Code) implementation supporting multiple SHA-2 variants.
//!
//! This module provides a flexible HMAC implementation that supports various SHA-2 hash functions
//! for generating and verifying message authentication codes. The implementation is thread-safe
//! and can be used in concurrent contexts.

use crate::results::AppResult;
use chrono::Utc;
use hmac::{Hmac as HHmac, Mac};
use sha2::{Sha224, Sha256, Sha384, Sha512, Sha512_224, Sha512_256};

/// Supported hash functions for HMAC generation and verification.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum HashFunc {
    /// SHA-224 hash function (224-bit output)
    Sha224,
    #[default]
    /// SHA-256 hash function (256-bit output)
    Sha256,
    /// SHA-384 hash function (384-bit output)
    Sha384,
    /// SHA-512 hash function (512-bit output)
    Sha512,
    /// SHA-512/224 hash function (224-bits output using SHA-512 internal state)
    Sha512224,
    /// SHA-512/256 hash function (256-bits output using SHA-512 internal state)
    Sha512256,
}

/// HMAC generator and verifier structure.
///
/// This structure provides methods for generating and verifying HMACs using
/// various SHA-2 hash functions. It is thread-safe and can be cloned.
#[derive(Clone)]
pub struct Hmac {
    /// Secret key used for HMAC generation and verification
    secret: String,
    /// Hash function used for HMAC generation and verification
    func: HashFunc,
}

impl Hmac {
    /// Creates a new HMAC instance with the specified secret key.
    ///
    /// # Arguments
    ///
    /// * `secret` - The secret key to use for HMAC operations
    ///
    /// # Example
    ///
    /// ```
    /// use foxtive::helpers::hmac::{Hmac, HashFunc};
    ///
    /// let hmac = Hmac::new("my_secret_key", HashFunc::Sha256);
    /// ```
    pub fn new(secret: &str, func: HashFunc) -> Self {
        Hmac {
            func,
            secret: secret.to_string(),
        }
    }

    /// Generates an HMAC for the given value using the specified hash function.
    ///
    /// # Arguments
    ///
    /// * `value` - The message to generate an HMAC for
    /// * `fun` - The hash function to use
    ///
    /// # Returns
    ///
    /// Returns a Result containing the hexadecimal string representation of the HMAC
    /// or an error if HMAC generation fails.
    ///
    /// # Example
    ///
    /// ```
    /// use foxtive::helpers::hmac::{Hmac, HashFunc};
    ///
    /// let hmac = Hmac::new("my_secret_key", HashFunc::Sha256);
    /// let value = "message".to_string();
    /// let hash = hmac.hash(&value).unwrap();
    /// ```
    pub fn hash(&self, value: &String) -> AppResult<String> {
        match self.func {
            HashFunc::Sha224 => {
                let mut mac = HHmac::<Sha224>::new_from_slice(self.secret.as_bytes())?;
                mac.update(value.as_bytes());
                Self::convert_to_string(mac.finalize().into_bytes().as_slice())
            }
            HashFunc::Sha256 => {
                let mut mac = HHmac::<Sha256>::new_from_slice(self.secret.as_bytes())?;
                mac.update(value.as_bytes());
                Self::convert_to_string(mac.finalize().into_bytes().as_slice())
            }
            HashFunc::Sha384 => {
                let mut mac = HHmac::<Sha384>::new_from_slice(self.secret.as_bytes())?;
                mac.update(value.as_bytes());
                Self::convert_to_string(mac.finalize().into_bytes().as_slice())
            }
            HashFunc::Sha512 => {
                let mut mac = HHmac::<Sha512>::new_from_slice(self.secret.as_bytes())?;
                mac.update(value.as_bytes());
                Self::convert_to_string(mac.finalize().into_bytes().as_slice())
            }
            HashFunc::Sha512224 => {
                let mut mac = HHmac::<Sha512_224>::new_from_slice(self.secret.as_bytes())?;
                mac.update(value.as_bytes());
                Self::convert_to_string(mac.finalize().into_bytes().as_slice())
            }
            HashFunc::Sha512256 => {
                let mut mac = HHmac::<Sha512_256>::new_from_slice(self.secret.as_bytes())?;
                mac.update(value.as_bytes());
                Self::convert_to_string(mac.finalize().into_bytes().as_slice())
            }
        }
    }

    /// Generates a random HMAC using the current timestamp as both the key and value.
    ///
    /// This method uses the default hash function (SHA-256) and the current timestamp
    /// to generate a random HMAC. This can be useful for generating unique tokens
    /// or identifiers.
    ///
    /// # Returns
    ///
    /// Returns a Result containing the generated random HMAC as a hexadecimal string
    /// or an error if generation fails.
    ///
    /// # Example
    ///
    /// ```
    /// use foxtive::helpers::hmac::Hmac;
    ///
    /// let random_hmac = Hmac::random().unwrap();
    /// ```
    pub fn random() -> AppResult<String> {
        let timestamp = Utc::now().timestamp_micros().to_string();
        // Using the default hash function (Sha256) for random generation
        Hmac::new(&timestamp, HashFunc::default()).hash(&timestamp)
    }

    /// Verifies an HMAC against a provided value using the specified hash function.
    ///
    /// # Arguments
    ///
    /// * `value` - The original message
    /// * `hash` - The HMAC to verify against
    /// * `fun` - The hash function to use
    ///
    /// # Returns
    ///
    /// Returns a Result containing a boolean indicating whether the HMAC is valid
    /// or an error if verification fails.
    ///
    /// # Example
    ///
    /// ```
    /// use foxtive::helpers::hmac::{Hmac, HashFunc};
    ///
    /// let hmac = Hmac::new("my_secret_key", HashFunc::Sha256);
    /// let value = "message".to_string();
    /// let hash = hmac.hash(&value).unwrap();
    ///
    /// assert!(hmac.verify(&value, &hash).unwrap());
    /// ```
    pub fn verify(&self, value: &String, hash: &String) -> AppResult<bool> {
        let computed = self.hash(value)?;
        Ok(hash == &computed)
    }

    /// Converts a byte slice to its hexadecimal string representation.
    ///
    /// # Arguments
    ///
    /// * `slices` - The byte slice to convert
    ///
    /// # Returns
    ///
    /// Returns a Result containing the hexadecimal string representation
    /// or an error if conversion fails.
    fn convert_to_string(slices: &[u8]) -> AppResult<String> {
        Ok(hex::encode(slices))
    }
}

#[cfg(test)]
mod tests {
    use super::{HashFunc, Hmac};

    #[test]
    fn test_hash() {
        let hmac = Hmac::new("mysecret", HashFunc::Sha256);
        let value = "my message".to_string();
        let expected_hmac = "6df7d0cf7d3a52a08acbd7c12a2ab86b15820de24a78bd51e264e257de3316b0";

        let generated_hmac = hmac.hash(&value).unwrap();

        assert_eq!(
            generated_hmac, expected_hmac,
            "The generated HMAC does not match the expected value."
        );
    }

    #[test]
    fn test_random() {
        let random_hmac1 = Hmac::random().unwrap();
        let random_hmac2 = Hmac::random().unwrap();

        assert_ne!(
            random_hmac1, random_hmac2,
            "The generated HMACs should be different."
        );
        assert!(
            !random_hmac1.is_empty() && !random_hmac2.is_empty(),
            "The generated HMACs should not be empty."
        );
    }

    #[test]
    fn test_hmac_valid() {
        let hmac = Hmac::new("mysecret", HashFunc::Sha256);
        let value = "my message".to_string();
        let provided_hmac =
            "6df7d0cf7d3a52a08acbd7c12a2ab86b15820de24a78bd51e264e257de3316b0".to_string();

        let is_valid = hmac.verify(&value, &provided_hmac).unwrap();

        assert!(
            is_valid,
            "The HMAC verification should succeed, but it failed."
        );
    }

    #[test]
    fn test_hmac_invalid() {
        let hmac = Hmac::new("mysecret", HashFunc::Sha256);
        let value = "my message".to_string();
        let provided_hmac = "invalidhmac".to_string();

        let is_valid = hmac.verify(&value, &provided_hmac).unwrap();

        assert!(
            !is_valid,
            "The HMAC verification should fail, but it succeeded."
        );
    }

    #[test]
    fn test_hash_with_different_values() {
        let hmac = Hmac::new("mysecret", HashFunc::Sha256);

        let value1 = "message1".to_string();
        let value2 = "message2".to_string();

        let hmac1 = hmac.hash(&value1).unwrap();
        let hmac2 = hmac.hash(&value2).unwrap();

        assert_ne!(
            hmac1, hmac2,
            "HMACs for different values should not be the same."
        );
    }

    #[test]
    fn test_hash_with_different_functions() {
        let hmac256 = Hmac::new("mysecret", HashFunc::Sha256);
        let hmac512 = Hmac::new("mysecret", HashFunc::Sha512);
        let value = "my message".to_string();

        let sha256_hmac = hmac256.hash(&value).unwrap();
        let sha512_hmac = hmac512.hash(&value).unwrap();

        assert_ne!(
            sha256_hmac, sha512_hmac,
            "HMACs with different hash functions should not be the same."
        );
    }

    #[test]
    fn test_verify_with_different_functions() {
        let hmac = Hmac::new("mysecret", HashFunc::Sha512);
        let value = "my message".to_string();

        // Generate HMAC with SHA-512
        let sha512_hmac = hmac.hash(&value).unwrap();

        // Verify should succeed with SHA-512
        assert!(
            hmac.verify(&value, &sha512_hmac).unwrap(),
            "Verification should succeed with matching hash function"
        );

        // Verify should fail with SHA-256
        let hmac = Hmac::new("mysecret", HashFunc::Sha256);
        assert!(
            !hmac.verify(&value, &sha512_hmac).unwrap(),
            "Verification should fail with different hash function"
        );
    }
}
