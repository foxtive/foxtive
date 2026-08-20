//! JWE (JSON Web Encryption) helper - RFC 7516.
//!
//! Provides encrypt/decrypt of arbitrary payloads using JWE Compact Serialization.
//! Keys are cached at construction time for reuse across calls.
//!
//! # Key Management Algorithms
//!
//! - **Symmetric:** `Dir`, `A128KW`, `A192KW`, `A256KW`
//! - **Asymmetric:** `RSA-OAEP-256`
//! - **Password-based:** `PBES2-HS256+A128KW`, `PBES2-HS384+A192KW`, `PBES2-HS512+A256KW`
//!
//! # Content Encryption Algorithms
//!
//! - `A128GCM`, `A192GCM`, `A256GCM`
//! - `A128CBC-HS256`, `A192CBC-HS384`, `A256CBC-HS512`
//!
//! # Example
//!
//! ```ignore
//! use foxtive::helpers::jwe::{Jwe, JweConfig, JweAlgorithm, JweEncryption};
//!
//! // Uses A256KW + A256GCM by default
//! let jwe = Jwe::from_symmetric(b"0123456789abcdef0123456789abcdef").unwrap();
//! let token = jwe.encrypt(b"secret payload").unwrap();
//! let plaintext = jwe.decrypt(&token).unwrap();
//!
//! // Override defaults via config
//! let config = JweConfig::symmetric(b"0123456789abcdef")
//!     .unwrap()
//!     .with_defaults(JweAlgorithm::A128KW, JweEncryption::A128GCM);
//! let jwe = Jwe::new(config);
//!
//! // Or override per-call
//! let token = jwe.encrypt_with(
//!     b"secret payload",
//!     JweAlgorithm::A128KW,
//!     JweEncryption::A128GCM,
//! ).unwrap();
//! ```

use serde::Serialize;
use serde::de::DeserializeOwned;

pub use jose_rs::algorithm::{JweAlgorithm, JweEncryption};

use crate::prelude::{AppMessage, AppResult};

/// Configuration for JWE encryption and decryption.
///
/// Holds the key material and default algorithms. Keys are stored
/// at construction and reused on every encrypt/decrypt call.
///
/// Default algorithms are `A256KW` (key management) and `A256GCM`
/// (content encryption). Use `with_defaults()` to override.
#[derive(Clone)]
pub struct JweConfig {
    /// cached key encryption/decryption key bytes
    key: Vec<u8>,
    /// default key management algorithm
    default_alg: JweAlgorithm,
    /// default content encryption algorithm
    default_enc: JweEncryption,
}

impl JweConfig {
    /// Create a JWE config from a shared symmetric key.
    ///
    /// The key is used directly for `Dir`, or as the Key Encryption Key
    /// for `A128KW` / `A192KW` / `A256KW` and PBES2 variants.
    ///
    /// # Key Size Requirements
    ///
    /// | Algorithm | Required key size |
    /// |-----------|------------------|
    /// | `A128KW`  | 16 bytes         |
    /// | `A192KW`  | 24 bytes         |
    /// | `A256KW`  | 32 bytes         |
    /// | `Dir`     | depends on `enc` |
    pub fn symmetric(key: &[u8]) -> AppResult<Self> {
        if key.is_empty() {
            return Err(AppMessage::Infrastructure {
                message: "JWE symmetric key must not be empty".into(),
                source: None,
            });
        }
        Ok(Self {
            key: key.to_vec(),
            default_alg: JweAlgorithm::A256KW,
            default_enc: JweEncryption::A256GCM,
        })
    }

    /// Override the default algorithms.
    pub fn with_defaults(mut self, alg: JweAlgorithm, enc: JweEncryption) -> Self {
        self.default_alg = alg;
        self.default_enc = enc;
        self
    }

    pub fn key(&self) -> &[u8] {
        &self.key
    }

    pub fn default_alg(&self) -> JweAlgorithm {
        self.default_alg
    }

    pub fn default_enc(&self) -> JweEncryption {
        self.default_enc
    }
}

/// JWE encryption helper with cached key material.
///
/// Follows the same design as [`Jwt`](crate::helpers::jwt::Jwt) - keys are
/// parsed once at construction and reused on every encrypt/decrypt call.
#[derive(Clone)]
pub struct Jwe {
    config: JweConfig,
}

impl Jwe {
    /// Create a JWE helper from a config.
    pub fn new(config: JweConfig) -> Self {
        Self { config }
    }

    /// Convenience constructor from a shared symmetric key.
    pub fn from_symmetric(key: &[u8]) -> AppResult<Self> {
        Ok(Self::new(JweConfig::symmetric(key)?))
    }

    pub fn config(&self) -> &JweConfig {
        &self.config
    }

    /// Encrypt a serializable payload using default algorithms.
    pub fn encrypt<T: Serialize>(&self, payload: &T) -> AppResult<String> {
        self.encrypt_with(payload, self.config.default_alg, self.config.default_enc)
    }

    /// Encrypt a serializable payload with specific algorithms.
    ///
    /// # Arguments
    ///
    /// * `payload` - any `Serialize` type; serialized to JSON before encryption
    /// * `alg` - key management algorithm
    /// * `enc` - content encryption algorithm
    pub fn encrypt_with<T: Serialize>(
        &self,
        payload: &T,
        alg: JweAlgorithm,
        enc: JweEncryption,
    ) -> AppResult<String> {
        let plaintext = serde_json::to_vec(payload).map_err(|e| AppMessage::Infrastructure {
            message: format!("JWE serialize failed: {e}"),
            source: Some(Box::new(e)),
        })?;

        jose_rs::jwe::compact::encrypt(&self.config.key, &plaintext, alg, enc).map_err(|e| {
            AppMessage::Infrastructure {
                message: format!("JWE encrypt failed: {e}"),
                source: Some(Box::new(e)),
            }
        })
    }

    /// Encrypt raw bytes using default algorithms.
    pub fn encrypt_raw(&self, plaintext: &[u8]) -> AppResult<String> {
        self.encrypt_raw_with(plaintext, self.config.default_alg, self.config.default_enc)
    }

    /// Encrypt raw bytes with specific algorithms.
    pub fn encrypt_raw_with(
        &self,
        plaintext: &[u8],
        alg: JweAlgorithm,
        enc: JweEncryption,
    ) -> AppResult<String> {
        jose_rs::jwe::compact::encrypt(&self.config.key, plaintext, alg, enc).map_err(|e| {
            AppMessage::Infrastructure {
                message: format!("JWE encrypt failed: {e}"),
                source: Some(Box::new(e)),
            }
        })
    }

    /// Decrypt a JWE token and deserialize into a typed payload.
    pub fn decrypt<T: DeserializeOwned>(&self, token: &str) -> AppResult<T> {
        let plaintext = jose_rs::jwe::compact::decrypt(&self.config.key, token).map_err(|e| {
            AppMessage::Infrastructure {
                message: format!("JWE decrypt failed: {e}"),
                source: Some(Box::new(e)),
            }
        })?;

        serde_json::from_slice(&plaintext).map_err(|e| AppMessage::Infrastructure {
            message: format!("JWE deserialize failed: {e}"),
            source: Some(Box::new(e)),
        })
    }

    /// Decrypt a JWE token and return raw bytes.
    pub fn decrypt_raw(&self, token: &str) -> AppResult<Vec<u8>> {
        jose_rs::jwe::compact::decrypt(&self.config.key, token).map_err(|e| {
            AppMessage::Infrastructure {
                message: format!("JWE decrypt failed: {e}"),
                source: Some(Box::new(e)),
            }
        })
    }

    /// Change the default key management algorithm.
    pub fn change_algorithm(&mut self, alg: JweAlgorithm) -> &mut Self {
        self.config.default_alg = alg;
        self
    }

    /// Change the default content encryption algorithm.
    pub fn change_encryption(&mut self, enc: JweEncryption) -> &mut Self {
        self.config.default_enc = enc;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestPayload {
        user_id: u64,
        role: String,
    }

    fn test_payload() -> TestPayload {
        TestPayload {
            user_id: 42,
            role: "admin".into(),
        }
    }

    // A256KW requires a 32-byte KEK
    fn symmetric_key() -> Vec<u8> {
        b"0123456789abcdef0123456789abcdef".to_vec()
    }

    #[test]
    fn test_encrypt_decrypt_symmetric_round_trip() {
        let config = JweConfig::symmetric(&symmetric_key())
            .unwrap()
            .with_defaults(JweAlgorithm::A256KW, JweEncryption::A256GCM);
        let jwe = Jwe::new(config);
        let payload = test_payload();

        let token = jwe.encrypt(&payload).unwrap();

        // compact serialization has 5 dot-separated parts
        assert_eq!(token.split('.').count(), 5);

        let decrypted: TestPayload = jwe.decrypt(&token).unwrap();
        assert_eq!(decrypted, payload);
    }

    #[test]
    fn test_encrypt_raw_decrypt_raw() {
        let jwe = Jwe::from_symmetric(&symmetric_key()).unwrap();
        let plaintext = b"hello world";

        let token = jwe
            .encrypt_raw_with(plaintext, JweAlgorithm::A256KW, JweEncryption::A256GCM)
            .unwrap();

        let decrypted = jwe.decrypt_raw(&token).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_invalid_token_fails() {
        let jwe = Jwe::from_symmetric(&symmetric_key()).unwrap();
        let result = jwe.decrypt_raw("not.a.valid.jwe");
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let jwe1 = Jwe::from_symmetric(&symmetric_key()).unwrap();
        let jwe2 = Jwe::from_symmetric(b"different-key-0123456789abcdef").unwrap();

        let token = jwe1
            .encrypt_raw_with(b"secret", JweAlgorithm::A256KW, JweEncryption::A256GCM)
            .unwrap();

        let result = jwe2.decrypt_raw(&token);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_key_rejected() {
        let result = JweConfig::symmetric(b"");
        assert!(result.is_err());
    }

    #[test]
    fn test_dir_with_a128gcm() {
        // Dir with A128GCM requires a 16-byte CEK
        let key = b"0123456789abcdef"; // 16 bytes
        let jwe = Jwe::from_symmetric(key).unwrap();

        let token = jwe
            .encrypt_raw_with(b"direct key", JweAlgorithm::Dir, JweEncryption::A128GCM)
            .unwrap();

        let decrypted = jwe.decrypt_raw(&token).unwrap();
        assert_eq!(decrypted, b"direct key");
    }

    #[test]
    fn test_a128kw_with_cbc() {
        // A128KW requires a 16-byte KEK
        let key = b"0123456789abcdef"; // 16 bytes
        let jwe = Jwe::from_symmetric(key).unwrap();

        let token = jwe
            .encrypt_raw_with(
                b"cbc payload",
                JweAlgorithm::A128KW,
                JweEncryption::A128CbcHs256,
            )
            .unwrap();

        let decrypted = jwe.decrypt_raw(&token).unwrap();
        assert_eq!(decrypted, b"cbc payload");
    }

    #[test]
    fn test_rsa_oaep_256_round_trip() {
        use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey};
        use rsa::{RsaPrivateKey, RsaPublicKey};

        let mut rng = rsa::rand_core::OsRng;
        let private_key = RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let public_key = RsaPublicKey::from(&private_key);

        // jose-rs expects SPKI DER for encrypt, PKCS#8 DER for decrypt
        let spki_der = public_key.to_public_key_der().unwrap().as_ref().to_vec();
        let pkcs8_der = private_key.to_pkcs8_der().unwrap().as_bytes().to_vec();

        // encrypt uses the public key (SPKI DER)
        let encrypter = Jwe::from_symmetric(&spki_der).unwrap();
        let token = encrypter
            .encrypt_raw_with(
                b"rsa secret",
                JweAlgorithm::RsaOaep256,
                JweEncryption::A256GCM,
            )
            .unwrap();

        // decrypt uses the private key (PKCS#8 DER)
        let decrypter = Jwe::from_symmetric(&pkcs8_der).unwrap();
        let plaintext = decrypter.decrypt_raw(&token).unwrap();
        assert_eq!(plaintext, b"rsa secret");
    }

    #[test]
    fn test_encrypt_uses_defaults() {
        let jwe = Jwe::from_symmetric(&symmetric_key()).unwrap();
        let token = jwe.encrypt(&test_payload()).unwrap();
        assert_eq!(token.split('.').count(), 5);

        let decrypted: TestPayload = jwe.decrypt(&token).unwrap();
        assert_eq!(decrypted, test_payload());
    }

    #[test]
    fn test_config_defaults() {
        let config = JweConfig::symmetric(&symmetric_key()).unwrap();

        assert_eq!(config.default_alg(), JweAlgorithm::A256KW);
        assert_eq!(config.default_enc(), JweEncryption::A256GCM);

        let config = config.with_defaults(JweAlgorithm::A128KW, JweEncryption::A128GCM);
        assert_eq!(config.default_alg(), JweAlgorithm::A128KW);
        assert_eq!(config.default_enc(), JweEncryption::A128GCM);
    }

    #[test]
    fn test_jwe_clone() {
        let config = JweConfig::symmetric(&symmetric_key())
            .unwrap()
            .with_defaults(JweAlgorithm::A256KW, JweEncryption::A256GCM);
        let jwe1 = Jwe::new(config);
        let jwe2 = jwe1.clone();

        let token = jwe1.encrypt(&test_payload()).unwrap();
        let decrypted: TestPayload = jwe2.decrypt(&token).unwrap();
        assert_eq!(decrypted, test_payload());
    }
}
