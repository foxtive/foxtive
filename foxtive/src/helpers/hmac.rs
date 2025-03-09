use crate::results::AppResult;
use chrono::Utc;
use hmac::{Hmac as HHmac, Mac};
use sha2::Sha256;

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

    pub fn hash(&self, value: &String) -> AppResult<String> {
        type HmacSha256 = HHmac<Sha256>;

        let mut mac = HmacSha256::new_from_slice(self.secret.as_bytes())?;

        mac.update(value.as_bytes());

        let result = mac.finalize();
        let code_bytes = result.into_bytes();
        Ok(hex::encode(code_bytes.as_slice()))
    }

    pub fn generate_random() -> AppResult<String> {
        let timestamp = Utc::now().timestamp_micros().to_string();
        Hmac::new(&timestamp).hash(&timestamp)
    }

    pub fn verify(&self, value: &String, provided_hmac: &String) -> AppResult<bool> {
        let mac = self.hash(value)?;
        Ok(provided_hmac == &mac)
    }
}

#[cfg(test)]
mod tests {
    use super::Hmac;

    #[test]
    fn test_hash() {
        let hmac = Hmac::new("mysecret");
        let value = "my message".to_string();
        let expected_hmac = "6df7d0cf7d3a52a08acbd7c12a2ab86b15820de24a78bd51e264e257de3316b0";

        let generated_hmac = hmac.hash(&value).unwrap();

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
        assert_eq!(
            random_hmac1.len(),
            64,
            "The generated HMAC should have a length of 64 characters."
        );
        assert_eq!(
            random_hmac2.len(),
            64,
            "The generated HMAC should have a length of 64 characters."
        );
    }

    #[test]
    fn test_hmac_valid() {
        let hmac = Hmac::new("mysecret");
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
        let hmac = Hmac::new("mysecret");
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
        let hmac = Hmac::new("mysecret");

        let value1 = "message1".to_string();
        let value2 = "message2".to_string();

        let hmac1 = hmac.hash(&value1).unwrap();
        let hmac2 = hmac.hash(&value2).unwrap();

        assert_ne!(
            hmac1, hmac2,
            "HMACs for different values should not be the same."
        );
    }
}
