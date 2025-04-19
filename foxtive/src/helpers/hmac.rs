use crate::results::AppResult;
use chrono::Utc;
use hmac::{Hmac as HHmac, Mac};
use sha2::{Sha224, Sha256, Sha384, Sha512, Sha512_224, Sha512_256};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum HashFunc {
    Sha224,
    Sha256,
    Sha384,
    Sha512,
    Sha512224,
    Sha512256,
}

impl Default for HashFunc {
    fn default() -> Self {
        HashFunc::Sha256
    }
}

#[derive(Clone)]
pub struct Hmac {
    secret: String,
}

impl Hmac {
    pub fn new(secret: &str) -> Self {
        Hmac {
            secret: secret.to_string(),
        }
    }

    pub fn hash(&self, value: &String, fun: HashFunc) -> AppResult<String> {
        match fun {
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

    fn convert_to_string(slices: &[u8]) -> AppResult<String> {
        Ok(hex::encode(slices))
    }

    pub fn generate_random() -> AppResult<String> {
        let timestamp = Utc::now().timestamp_micros().to_string();
        // Using default hash function (Sha256) for random generation
        Hmac::new(&timestamp).hash(&timestamp, HashFunc::default())
    }

    pub fn verify(&self, value: &String, hash: &String, fun: HashFunc) -> AppResult<bool> {
        let computed = self.hash(value, fun)?;
        Ok(hash == &computed)
    }
}

#[cfg(test)]
mod tests {
    use super::{HashFunc, Hmac};

    #[test]
    fn test_hash() {
        let hmac = Hmac::new("mysecret");
        let value = "my message".to_string();
        let expected_hmac = "6df7d0cf7d3a52a08acbd7c12a2ab86b15820de24a78bd51e264e257de3316b0";

        let generated_hmac = hmac.hash(&value, HashFunc::Sha256).unwrap();

        assert_eq!(
            generated_hmac, expected_hmac,
            "The generated HMAC does not match the expected value."
        );
    }

    #[test]
    fn test_generate_random() {
        let random_hmac1 = Hmac::generate_random().unwrap();
        let random_hmac2 = Hmac::generate_random().unwrap();

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
        let hmac = Hmac::new("mysecret");
        let value = "my message".to_string();
        let provided_hmac =
            "6df7d0cf7d3a52a08acbd7c12a2ab86b15820de24a78bd51e264e257de3316b0".to_string();

        let is_valid = hmac
            .verify(&value, &provided_hmac, HashFunc::Sha256)
            .unwrap();

        assert!(
            is_valid,
            "The HMAC verification should succeed, but it failed."
        );
    }

    #[test]
    fn test_hmac_invalid() {
        let hmac = Hmac::new("mysecret");
        let value = "my message".to_string();
        let provided_hmac = "invalidhmac".to_string();

        let is_valid = hmac
            .verify(&value, &provided_hmac, HashFunc::Sha256)
            .unwrap();

        assert!(
            !is_valid,
            "The HMAC verification should fail, but it succeeded."
        );
    }

    #[test]
    fn test_hash_with_different_values() {
        let hmac = Hmac::new("mysecret");

        let value1 = "message1".to_string();
        let value2 = "message2".to_string();

        let hmac1 = hmac.hash(&value1, HashFunc::Sha256).unwrap();
        let hmac2 = hmac.hash(&value2, HashFunc::Sha256).unwrap();

        assert_ne!(
            hmac1, hmac2,
            "HMACs for different values should not be the same."
        );
    }

    #[test]
    fn test_hash_with_different_functions() {
        let hmac = Hmac::new("mysecret");
        let value = "my message".to_string();

        let sha256_hmac = hmac.hash(&value, HashFunc::Sha256).unwrap();
        let sha512_hmac = hmac.hash(&value, HashFunc::Sha512).unwrap();

        assert_ne!(
            sha256_hmac, sha512_hmac,
            "HMACs with different hash functions should not be the same."
        );
    }

    #[test]
    fn test_verify_with_different_functions() {
        let hmac = Hmac::new("mysecret");
        let value = "my message".to_string();

        // Generate HMAC with SHA-512
        let sha512_hmac = hmac.hash(&value, HashFunc::Sha512).unwrap();

        // Verify should succeed with SHA-512
        assert!(
            hmac.verify(&value, &sha512_hmac, HashFunc::Sha512).unwrap(),
            "Verification should succeed with matching hash function"
        );

        // Verify should fail with SHA-256
        assert!(
            !hmac.verify(&value, &sha512_hmac, HashFunc::Sha256).unwrap(),
            "Verification should fail with different hash function"
        );
    }
}
