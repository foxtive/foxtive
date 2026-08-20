//! JWT (JSON Web Token) helper - RFC 7519.
//!
//! Provides token generation and validation with support for multiple key types:
//! - **RSA** (RS256, RS384, RS512) - asymmetric, PEM-encoded keys
//! - **HMAC** (HS256, HS384, HS512) - symmetric, shared secret
//!
//! # Microservice Key Distribution
//!
//! In a multi-service architecture, only the **auth service** should hold the
//! private key for signing. All other services use [`JwtVerifier`] which only
//! requires the public key - the private key never leaves the auth service.
//!
//! ```ignore
//! // === Auth service (signs + verifies) ===
//! let config = JwtConfig::rsa_pem(public_pem, private_pem, 60)?;
//! let jwt = Jwt::new(config);
//! let token = jwt.generate(claims)?;
//!
//! // === Other services (verify only) ===
//! let verifier = JwtVerifier::from_rsa_public_key(public_pem)?;
//! let decoded = verifier.decode::<Claims>(&token, &validation)?;
//! ```
//!
//! # Standard Usage
//!
//! ```ignore
//! use foxtive::helpers::jwt::{Jwt, JwtConfig};
//!
//! // RSA-based JWT (asymmetric)
//! let config = JwtConfig::rsa_pem(public_pem, private_pem, 60)?;
//! let jwt = Jwt::new(config);
//!
//! // HMAC-based JWT (symmetric)
//! let config = JwtConfig::hmac(b"my-secret-key", 60);
//! let jwt = Jwt::new(config);
//! ```

use jsonwebtoken::{DecodingKey, EncodingKey, Header, TokenData};
use jsonwebtoken::{decode, encode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub use jsonwebtoken::{Algorithm as JwtAlgorithm, Validation};

use crate::prelude::{AppMessage, AppResult};

/// Configuration for JWT token generation and validation.
///
/// Holds the key material and default settings. Keys are parsed once at
/// construction and cached for reuse.
///
/// # Signing Key
///
/// `JwtConfig` always requires a signing (private) key - this is enforced at
/// construction time. Services that only need to **verify** tokens (no private
/// key) should use [`JwtVerifier`] instead.
#[derive(Clone)]
pub struct JwtConfig {
    /// cached encoding key for token signing
    encoding_key: EncodingKey,
    /// cached decoding key for token verification
    decoding_key: DecodingKey,
    /// default algorithm for token generation
    algorithm: JwtAlgorithm,
    /// token lifetime in minutes
    token_lifetime: i64,
}

impl JwtConfig {
    /// Create a JWT config from RSA PEM-encoded keys.
    ///
    /// # Arguments
    ///
    /// * `public_pem` - PEM-encoded public key for verification
    /// * `private_pem` - PEM-encoded private key for signing
    /// * `token_lifetime` - Token lifetime in minutes
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = JwtConfig::rsa_pem(public_pem, private_pem, 60)?;
    /// ```
    pub fn rsa_pem(public_pem: &str, private_pem: &str, token_lifetime: i64) -> AppResult<Self> {
        let encoding_key = EncodingKey::from_rsa_pem(private_pem.as_bytes()).map_err(|e| {
            AppMessage::Infrastructure {
                message: format!("Failed to parse RSA private key for JWT: {e}"),
                source: Some(Box::new(e)),
            }
        })?;
        let decoding_key = DecodingKey::from_rsa_pem(public_pem.as_bytes()).map_err(|e| {
            AppMessage::Infrastructure {
                message: format!("Failed to parse RSA public key for JWT: {e}"),
                source: Some(Box::new(e)),
            }
        })?;

        Ok(Self {
            encoding_key,
            decoding_key,
            algorithm: JwtAlgorithm::RS256,
            token_lifetime,
        })
    }

    /// Create a JWT config from RSA PEM-encoded keys with a specific algorithm.
    ///
    /// # Arguments
    ///
    /// * `public_pem` - PEM-encoded public key for verification
    /// * `private_pem` - PEM-encoded private key for signing
    /// * `algorithm` - RSA algorithm (RS256, RS384, or RS512)
    /// * `token_lifetime` - Token lifetime in minutes
    pub fn rsa_pem_with_algorithm(
        public_pem: &str,
        private_pem: &str,
        algorithm: JwtAlgorithm,
        token_lifetime: i64,
    ) -> AppResult<Self> {
        if !matches!(
            algorithm,
            JwtAlgorithm::RS256 | JwtAlgorithm::RS384 | JwtAlgorithm::RS512
        ) {
            return Err(AppMessage::Infrastructure {
                message: format!("Invalid RSA algorithm: {algorithm:?}"),
                source: None,
            });
        }

        let encoding_key = EncodingKey::from_rsa_pem(private_pem.as_bytes()).map_err(|e| {
            AppMessage::Infrastructure {
                message: format!("Failed to parse RSA private key for JWT: {e}"),
                source: Some(Box::new(e)),
            }
        })?;
        let decoding_key = DecodingKey::from_rsa_pem(public_pem.as_bytes()).map_err(|e| {
            AppMessage::Infrastructure {
                message: format!("Failed to parse RSA public key for JWT: {e}"),
                source: Some(Box::new(e)),
            }
        })?;

        Ok(Self {
            encoding_key,
            decoding_key,
            algorithm,
            token_lifetime,
        })
    }

    /// Create a JWT config from an HMAC shared secret.
    ///
    /// Uses HS256 algorithm by default.
    ///
    /// # Arguments
    ///
    /// * `secret` - Shared secret key bytes
    /// * `token_lifetime` - Token lifetime in minutes
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = JwtConfig::hmac(b"my-secret-key", 60);
    /// ```
    pub fn hmac(secret: &[u8], token_lifetime: i64) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret),
            decoding_key: DecodingKey::from_secret(secret),
            algorithm: JwtAlgorithm::HS256,
            token_lifetime,
        }
    }

    /// Create a JWT config from an HMAC shared secret with a specific algorithm.
    ///
    /// # Arguments
    ///
    /// * `secret` - Shared secret key bytes
    /// * `algorithm` - HMAC algorithm (HS256, HS384, or HS512)
    /// * `token_lifetime` - Token lifetime in minutes
    pub fn hmac_with_algorithm(
        secret: &[u8],
        algorithm: JwtAlgorithm,
        token_lifetime: i64,
    ) -> AppResult<Self> {
        // Validate that the algorithm is HMAC-based
        if !matches!(
            algorithm,
            JwtAlgorithm::HS256 | JwtAlgorithm::HS384 | JwtAlgorithm::HS512
        ) {
            return Err(AppMessage::Infrastructure {
                message: format!("Invalid HMAC algorithm: {algorithm:?}"),
                source: None,
            });
        }

        Ok(Self {
            encoding_key: EncodingKey::from_secret(secret),
            decoding_key: DecodingKey::from_secret(secret),
            algorithm,
            token_lifetime,
        })
    }

    pub fn algorithm(&self) -> JwtAlgorithm {
        self.algorithm
    }

    pub fn token_lifetime(&self) -> i64 {
        self.token_lifetime
    }

    /// Override the signing algorithm.
    pub fn with_algorithm(mut self, algorithm: JwtAlgorithm) -> Self {
        self.algorithm = algorithm;
        self
    }
}

/// JWT token helper with cached keys.
///
/// Follows the same design as [`Jwe`](crate::helpers::jwe::Jwe) - keys are
/// parsed once at construction and reused on every generate/decode call.
#[derive(Clone)]
pub struct Jwt {
    config: JwtConfig,
}

/// Standard JWT claims structure.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtTokenClaims {
    /// Identifies the subject (user or entity) the token is about.
    pub sub: String,
    /// Indicates when the token was issued (timestamp).
    pub iat: usize,
    /// Specifies when the token expires.
    pub exp: usize,
    /// Identifies the entity that issued the token.
    pub iss: String,
    /// Defines the intended recipient of the token.
    pub aud: String,
    /// A unique identifier for the token.
    pub jti: String,
}

/// Response structure containing the generated token.
#[derive(Serialize, Debug)]
pub struct AuthTokenData {
    /// The generated access token.
    pub access_token: String,
    /// Token type (typically "bearer").
    pub token_type: String,
    /// Token lifetime in minutes.
    pub expires_in: i64,
}

impl Jwt {
    /// Create a new JWT helper from a config.
    pub fn new(config: JwtConfig) -> Self {
        Self { config }
    }

    /// Convenience constructor from RSA PEM keys.
    pub fn from_rsa_pem(
        public_pem: &str,
        private_pem: &str,
        token_lifetime: i64,
    ) -> AppResult<Self> {
        Ok(Self::new(JwtConfig::rsa_pem(
            public_pem,
            private_pem,
            token_lifetime,
        )?))
    }

    /// Convenience constructor from HMAC secret.
    pub fn from_hmac(secret: &[u8], token_lifetime: i64) -> Self {
        Self::new(JwtConfig::hmac(secret, token_lifetime))
    }

    pub fn config(&self) -> &JwtConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut JwtConfig {
        &mut self.config
    }

    /// Change the signing algorithm.
    pub fn change_algorithm(&mut self, algorithm: JwtAlgorithm) -> &mut Self {
        self.config.algorithm = algorithm;
        self
    }

    /// Generates a JWT token with the given claims.
    ///
    /// Uses the algorithm configured in the `JwtConfig`.
    ///
    /// # Arguments
    ///
    /// * `claims` - The claims to encode into the token
    ///
    /// # Returns
    ///
    /// Returns `AppResult<AuthTokenData>` containing the access token, token type, and expiration.
    pub fn generate<C: Serialize>(&self, claims: C) -> AppResult<AuthTokenData> {
        self.generate_with_algorithm(claims, self.config.algorithm)
    }

    /// Generates a JWT token with a specific algorithm override.
    ///
    /// The algorithm must be compatible with the key type configured in `JwtConfig`.
    /// For RSA keys, use RS256/RS384/RS512. For HMAC keys, use HS256/HS384/HS512.
    ///
    /// # Arguments
    ///
    /// * `claims` - The claims to encode into the token
    /// * `algorithm` - The signing algorithm to use (overrides config default)
    pub fn generate_with_algorithm<C: Serialize>(
        &self,
        claims: C,
        algorithm: JwtAlgorithm,
    ) -> AppResult<AuthTokenData> {
        let token_header = Header::new(algorithm);
        let token = encode(&token_header, &claims, &self.config.encoding_key).map_err(|e| {
            AppMessage::Infrastructure {
                message: format!("JWT encode failed: {e}"),
                source: Some(Box::new(e)),
            }
        })?;

        Ok(AuthTokenData {
            access_token: token,
            token_type: "bearer".to_string(),
            expires_in: self.config.token_lifetime,
        })
    }

    /// Decodes and validates a JWT token.
    ///
    /// # Arguments
    ///
    /// * `token` - The JWT token string to decode
    /// * `validation` - Validation configuration (e.g., `Validation::new(JwtAlgorithm::RS256)`)
    ///
    /// # Returns
    ///
    /// Returns `AppResult<TokenData<C>>` containing the decoded claims.
    pub fn decode<C: DeserializeOwned + Clone>(
        &self,
        token: &str,
        validation: &Validation,
    ) -> AppResult<TokenData<C>> {
        decode::<C>(token, &self.config.decoding_key, validation).map_err(|e| {
            AppMessage::Infrastructure {
                message: format!("JWT decode failed: {e}"),
                source: Some(Box::new(e)),
            }
        })
    }

    /// Returns sample RSA keys for testing purposes.
    ///
    /// # Returns
    /// `(public_key, private_key)` - PEM-encoded RSA key pair.
    ///
    /// # Availability
    /// Only available in test builds or when the `test-utils` feature is enabled.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn dummy_keys() -> (String, String) {
        use rsa::RsaPrivateKey;
        use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey};

        let mut rng = rsa::rand_core::OsRng;
        let private_key =
            RsaPrivateKey::new(&mut rng, 2048).expect("Failed to generate RSA key pair");

        let public_key = private_key
            .to_public_key()
            .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
            .expect("Failed to extract public key");
        let private_key = private_key
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .expect("Failed to extract private key")
            .to_string();

        (public_key, private_key)
    }
}

// Verification-only JWT helper

/// Verification-only JWT helper for microservice deployments.
///
/// `JwtVerifier` only holds a decoding (public) key and exposes the `decode()`
/// method. Unlike [`Jwt`], there is no `generate()` method - the type system
/// prevents accidental token signing in services that should only verify.
///
/// # When to use
///
/// In a multi-service architecture, only the **auth service** holds the private
/// key. All other services should use `JwtVerifier` to validate incoming tokens
/// without ever having access to the signing key.
///
/// # Example
///
/// ```ignore
/// use foxtive::helpers::jwt::{JwtVerifier, Validation, JwtAlgorithm};
///
/// // Create from RSA public key only - no private key needed
/// let verifier = JwtVerifier::from_rsa_public_key(public_pem)?;
///
/// // Decode and verify a token
/// let mut validation = Validation::new(JwtAlgorithm::RS256);
/// validation.set_audience(&["my-service"]);
/// let decoded = verifier.decode::<MyClaims>(&token, &validation)?;
/// ```
#[derive(Clone)]
pub struct JwtVerifier {
    decoding_key: DecodingKey,
    algorithm: JwtAlgorithm,
}

impl JwtVerifier {
    /// Create a verifier from an RSA PEM-encoded public key.
    ///
    /// Uses RS256 as the default algorithm.
    ///
    /// # Arguments
    ///
    /// * `public_pem` - PEM-encoded public key for verification
    pub fn from_rsa_public_key(public_pem: &str) -> AppResult<Self> {
        let decoding_key = DecodingKey::from_rsa_pem(public_pem.as_bytes()).map_err(|e| {
            AppMessage::Infrastructure {
                message: format!("Failed to parse RSA public key for JWT verification: {e}"),
                source: Some(Box::new(e)),
            }
        })?;

        Ok(Self {
            decoding_key,
            algorithm: JwtAlgorithm::RS256,
        })
    }

    /// Create a verifier from an RSA public key with a specific algorithm.
    ///
    /// # Arguments
    ///
    /// * `public_pem` - PEM-encoded public key for verification
    /// * `algorithm` - RSA algorithm (RS256, RS384, or RS512)
    pub fn from_rsa_public_key_with_algorithm(
        public_pem: &str,
        algorithm: JwtAlgorithm,
    ) -> AppResult<Self> {
        if !matches!(
            algorithm,
            JwtAlgorithm::RS256 | JwtAlgorithm::RS384 | JwtAlgorithm::RS512
        ) {
            return Err(AppMessage::Infrastructure {
                message: format!("Invalid RSA algorithm: {algorithm:?}"),
                source: None,
            });
        }

        let decoding_key = DecodingKey::from_rsa_pem(public_pem.as_bytes()).map_err(|e| {
            AppMessage::Infrastructure {
                message: format!("Failed to parse RSA public key for JWT verification: {e}"),
                source: Some(Box::new(e)),
            }
        })?;

        Ok(Self {
            decoding_key,
            algorithm,
        })
    }

    /// Create a verifier from an HMAC shared secret.
    ///
    /// Uses HS256 as the default algorithm.
    ///
    /// # Arguments
    ///
    /// * `secret` - Shared secret key bytes
    pub fn from_hmac(secret: &[u8]) -> Self {
        Self {
            decoding_key: DecodingKey::from_secret(secret),
            algorithm: JwtAlgorithm::HS256,
        }
    }

    /// Create a verifier from an HMAC shared secret with a specific algorithm.
    ///
    /// # Arguments
    ///
    /// * `secret` - Shared secret key bytes
    /// * `algorithm` - HMAC algorithm (HS256, HS384, or HS512)
    pub fn from_hmac_with_algorithm(secret: &[u8], algorithm: JwtAlgorithm) -> AppResult<Self> {
        if !matches!(
            algorithm,
            JwtAlgorithm::HS256 | JwtAlgorithm::HS384 | JwtAlgorithm::HS512
        ) {
            return Err(AppMessage::Infrastructure {
                message: format!("Invalid HMAC algorithm: {algorithm:?}"),
                source: None,
            });
        }

        Ok(Self {
            decoding_key: DecodingKey::from_secret(secret),
            algorithm,
        })
    }

    /// Create a verifier from a [`JwtConfig`] (signing key ignored).
    pub fn from_config(config: &JwtConfig) -> Self {
        Self {
            decoding_key: config.decoding_key.clone(),
            algorithm: config.algorithm,
        }
    }

    /// Decodes and validates a JWT token.
    ///
    /// # Arguments
    ///
    /// * `token` - The JWT token string to decode
    /// * `validation` - Validation configuration (e.g., `Validation::new(JwtAlgorithm::RS256)`)
    ///
    /// # Returns
    ///
    /// Returns `AppResult<TokenData<C>>` containing the decoded claims.
    pub fn decode<C: DeserializeOwned + Clone>(
        &self,
        token: &str,
        validation: &Validation,
    ) -> AppResult<TokenData<C>> {
        decode::<C>(token, &self.decoding_key, validation).map_err(|e| AppMessage::Infrastructure {
            message: format!("JWT decode failed: {e}"),
            source: Some(Box::new(e)),
        })
    }

    pub fn algorithm(&self) -> JwtAlgorithm {
        self.algorithm
    }
}

// Combined JWT + JWE API (sign-then-encrypt / decrypt-then-verify)

#[cfg(feature = "jwe")]
mod jwe_combined {
    use super::*;
    use crate::helpers::jwe::Jwe;

    impl Jwt {
        /// Generate a signed JWT and encrypt it with JWE in one step.
        ///
        /// The JWT is signed with the configured algorithm, serialized, then
        /// encrypted using the provided [`Jwe`] instance (which must have
        /// default algorithms set).
        ///
        /// Returns the JWE compact serialization string.
        ///
        /// # Arguments
        ///
        /// * `claims` - The claims to encode and sign
        /// * `jwe` - JWE helper with default algorithms configured
        pub fn generate_encrypted<C: Serialize>(&self, claims: C, jwe: &Jwe) -> AppResult<String> {
            let token_data = self.generate(claims)?;
            jwe.encrypt(&token_data.access_token)
        }

        /// Generate a signed JWT with a specific algorithm and encrypt it with JWE.
        ///
        /// # Arguments
        ///
        /// * `claims` - The claims to encode and sign
        /// * `algorithm` - Override signing algorithm
        /// * `jwe` - JWE helper with default algorithms configured
        pub fn generate_encrypted_with_algorithm<C: Serialize>(
            &self,
            claims: C,
            algorithm: JwtAlgorithm,
            jwe: &Jwe,
        ) -> AppResult<String> {
            let token_data = self.generate_with_algorithm(claims, algorithm)?;
            jwe.encrypt(&token_data.access_token)
        }

        /// Decrypt a JWE token and verify the inner JWT in one step.
        ///
        /// The JWE is decrypted to recover the JWT string, which is then
        /// validated and decoded using the provided [`Validation`].
        ///
        /// # Arguments
        ///
        /// * `jwe_token` - JWE compact serialization string
        /// * `jwe` - JWE helper (must use the same key used for encryption)
        /// * `validation` - JWT validation config (algorithm, audience, etc.)
        pub fn decode_decrypted<C: DeserializeOwned + Clone>(
            &self,
            jwe_token: &str,
            jwe: &Jwe,
            validation: &Validation,
        ) -> AppResult<TokenData<C>> {
            let jwt_string: String = jwe.decrypt(jwe_token)?;
            self.decode::<C>(&jwt_string, validation)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sample_claims() -> JwtTokenClaims {
        JwtTokenClaims {
            sub: "test_subject".to_string(),
            iat: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as usize,
            exp: (SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600) as usize,
            iss: "test_issuer".to_string(),
            aud: "test_audience".to_string(),
            jti: "test_jti".to_string(),
        }
    }

    // RSA tests

    #[test]
    fn test_rsa_pem_round_trip() {
        let (public_pem, private_pem) = Jwt::dummy_keys();
        let jwt = Jwt::from_rsa_pem(&public_pem, &private_pem, 60).unwrap();

        let claims = sample_claims();
        let token = jwt.generate(claims.clone()).unwrap();

        assert_eq!(token.token_type, "bearer");
        assert_eq!(token.expires_in, 60);

        let mut validation = Validation::new(JwtAlgorithm::RS256);
        validation.set_audience(&["test_audience"]);

        let decoded = jwt
            .decode::<JwtTokenClaims>(&token.access_token, &validation)
            .unwrap();
        assert_eq!(decoded.claims.sub, claims.sub);
        assert_eq!(decoded.claims.iss, claims.iss);
    }

    #[test]
    fn test_rsa_config_builder() {
        let (public_pem, private_pem) = Jwt::dummy_keys();
        let config = JwtConfig::rsa_pem(&public_pem, &private_pem, 30).unwrap();

        assert_eq!(config.algorithm(), JwtAlgorithm::RS256);
        assert_eq!(config.token_lifetime(), 30);

        let jwt = Jwt::new(config);
        let token = jwt.generate(sample_claims()).unwrap();
        assert!(!token.access_token.is_empty());
    }

    // HMAC tests

    #[test]
    fn test_hmac_round_trip() {
        let secret = b"my-super-secret-key";
        let jwt = Jwt::from_hmac(secret, 60);

        let claims = sample_claims();
        let token = jwt.generate(claims.clone()).unwrap();

        let mut validation = Validation::new(JwtAlgorithm::HS256);
        validation.set_audience(&["test_audience"]);

        let decoded = jwt
            .decode::<JwtTokenClaims>(&token.access_token, &validation)
            .unwrap();
        assert_eq!(decoded.claims.sub, claims.sub);
    }

    #[test]
    fn test_hmac_with_algorithm_hs384() {
        let secret = b"another-secret";
        let config = JwtConfig::hmac_with_algorithm(secret, JwtAlgorithm::HS384, 45).unwrap();

        assert_eq!(config.algorithm(), JwtAlgorithm::HS384);

        let jwt = Jwt::new(config);
        let token = jwt.generate(sample_claims()).unwrap();

        let mut validation = Validation::new(JwtAlgorithm::HS384);
        validation.set_audience(&["test_audience"]);

        let decoded = jwt
            .decode::<JwtTokenClaims>(&token.access_token, &validation)
            .unwrap();
        assert_eq!(decoded.claims.sub, "test_subject");
    }

    #[test]
    fn test_hmac_with_algorithm_hs512() {
        let secret = b"yet-another-secret";
        let config = JwtConfig::hmac_with_algorithm(secret, JwtAlgorithm::HS512, 90).unwrap();

        assert_eq!(config.algorithm(), JwtAlgorithm::HS512);

        let jwt = Jwt::new(config);
        let token = jwt.generate(sample_claims()).unwrap();

        let mut validation = Validation::new(JwtAlgorithm::HS512);
        validation.set_audience(&["test_audience"]);

        let decoded = jwt
            .decode::<JwtTokenClaims>(&token.access_token, &validation)
            .unwrap();
        assert_eq!(decoded.claims.iss, "test_issuer");
    }

    #[test]
    fn test_hmac_invalid_algorithm_rejected() {
        let secret = b"secret";
        let result = JwtConfig::hmac_with_algorithm(secret, JwtAlgorithm::RS256, 60);
        assert!(result.is_err());
    }

    // Error cases

    #[test]
    fn test_decode_invalid_token_fails() {
        let jwt = Jwt::from_hmac(b"secret", 60);
        let result = jwt
            .decode::<JwtTokenClaims>("invalid.token.here", &Validation::new(JwtAlgorithm::HS256));
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_wrong_secret_fails() {
        let jwt1 = Jwt::from_hmac(b"secret-one", 60);
        let jwt2 = Jwt::from_hmac(b"secret-two", 60);

        let token = jwt1.generate(sample_claims()).unwrap();

        let result = jwt2
            .decode::<JwtTokenClaims>(&token.access_token, &Validation::new(JwtAlgorithm::HS256));
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_wrong_algorithm_fails() {
        let jwt = Jwt::from_hmac(b"secret", 60);
        let token = jwt.generate(sample_claims()).unwrap();

        // Try to decode with wrong algorithm
        let result = jwt
            .decode::<JwtTokenClaims>(&token.access_token, &Validation::new(JwtAlgorithm::RS256));
        assert!(result.is_err());
    }

    // Clone tests

    #[test]
    fn test_jwt_clone() {
        let jwt1 = Jwt::from_hmac(b"clone-secret", 60);
        let jwt2 = jwt1.clone();

        let token = jwt1.generate(sample_claims()).unwrap();

        let mut validation = Validation::new(JwtAlgorithm::HS256);
        validation.set_audience(&["test_audience"]);

        let decoded = jwt2
            .decode::<JwtTokenClaims>(&token.access_token, &validation)
            .unwrap();
        assert_eq!(decoded.claims.sub, "test_subject");
    }

    // JwtAlgorithm config tests

    #[test]
    fn test_rsa_pem_with_algorithm_rs384() {
        let (public_pem, private_pem) = Jwt::dummy_keys();
        let config =
            JwtConfig::rsa_pem_with_algorithm(&public_pem, &private_pem, JwtAlgorithm::RS384, 30)
                .unwrap();

        assert_eq!(config.algorithm(), JwtAlgorithm::RS384);

        let jwt = Jwt::new(config);
        let token = jwt.generate(sample_claims()).unwrap();

        let mut validation = Validation::new(JwtAlgorithm::RS384);
        validation.set_audience(&["test_audience"]);

        let decoded = jwt
            .decode::<JwtTokenClaims>(&token.access_token, &validation)
            .unwrap();
        assert_eq!(decoded.claims.sub, "test_subject");
    }

    #[test]
    fn test_rsa_pem_with_algorithm_rs512() {
        let (public_pem, private_pem) = Jwt::dummy_keys();
        let config =
            JwtConfig::rsa_pem_with_algorithm(&public_pem, &private_pem, JwtAlgorithm::RS512, 60)
                .unwrap();

        assert_eq!(config.algorithm(), JwtAlgorithm::RS512);

        let jwt = Jwt::new(config);
        let token = jwt.generate(sample_claims()).unwrap();

        let mut validation = Validation::new(JwtAlgorithm::RS512);
        validation.set_audience(&["test_audience"]);

        let decoded = jwt
            .decode::<JwtTokenClaims>(&token.access_token, &validation)
            .unwrap();
        assert_eq!(decoded.claims.iss, "test_issuer");
    }

    #[test]
    fn test_rsa_pem_with_invalid_algorithm_rejected() {
        let (public_pem, private_pem) = Jwt::dummy_keys();
        let result =
            JwtConfig::rsa_pem_with_algorithm(&public_pem, &private_pem, JwtAlgorithm::HS256, 60);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_with_algorithm_builder() {
        let config = JwtConfig::hmac(b"secret", 60).with_algorithm(JwtAlgorithm::HS512);
        assert_eq!(config.algorithm(), JwtAlgorithm::HS512);

        let jwt = Jwt::new(config);
        let token = jwt.generate(sample_claims()).unwrap();

        let mut validation = Validation::new(JwtAlgorithm::HS512);
        validation.set_audience(&["test_audience"]);

        let decoded = jwt
            .decode::<JwtTokenClaims>(&token.access_token, &validation)
            .unwrap();
        assert_eq!(decoded.claims.sub, "test_subject");
    }

    // JwtVerifier tests

    #[test]
    fn test_verifier_from_rsa_public_key() {
        let (public_pem, private_pem) = Jwt::dummy_keys();

        // Auth service signs
        let signer = Jwt::from_rsa_pem(&public_pem, &private_pem, 60).unwrap();
        let token = signer.generate(sample_claims()).unwrap();

        // Other service verifies with JwtVerifier
        let verifier = JwtVerifier::from_rsa_public_key(&public_pem).unwrap();
        let mut validation = Validation::new(JwtAlgorithm::RS256);
        validation.set_audience(&["test_audience"]);

        let decoded = verifier
            .decode::<JwtTokenClaims>(&token.access_token, &validation)
            .unwrap();
        assert_eq!(decoded.claims.sub, "test_subject");
        assert_eq!(decoded.claims.iss, "test_issuer");
    }

    #[test]
    fn test_verifier_from_rsa_with_algorithm() {
        let (public_pem, private_pem) = Jwt::dummy_keys();

        let config =
            JwtConfig::rsa_pem_with_algorithm(&public_pem, &private_pem, JwtAlgorithm::RS384, 60)
                .unwrap();
        let signer = Jwt::new(config);
        let token = signer.generate(sample_claims()).unwrap();

        let verifier =
            JwtVerifier::from_rsa_public_key_with_algorithm(&public_pem, JwtAlgorithm::RS384)
                .unwrap();
        assert_eq!(verifier.algorithm(), JwtAlgorithm::RS384);

        let mut validation = Validation::new(JwtAlgorithm::RS384);
        validation.set_audience(&["test_audience"]);
        let decoded = verifier
            .decode::<JwtTokenClaims>(&token.access_token, &validation)
            .unwrap();
        assert_eq!(decoded.claims.sub, "test_subject");
    }

    #[test]
    fn test_verifier_from_hmac() {
        let secret = b"shared-secret";
        let signer = Jwt::from_hmac(secret, 60);
        let token = signer.generate(sample_claims()).unwrap();

        let verifier = JwtVerifier::from_hmac(secret);
        let mut validation = Validation::new(JwtAlgorithm::HS256);
        validation.set_audience(&["test_audience"]);

        let decoded = verifier
            .decode::<JwtTokenClaims>(&token.access_token, &validation)
            .unwrap();
        assert_eq!(decoded.claims.sub, "test_subject");
    }

    #[test]
    fn test_verifier_from_config() {
        let (public_pem, private_pem) = Jwt::dummy_keys();
        let config = JwtConfig::rsa_pem(&public_pem, &private_pem, 60).unwrap();

        let signer = Jwt::new(config.clone());
        let token = signer.generate(sample_claims()).unwrap();

        let verifier = JwtVerifier::from_config(&config);
        let mut validation = Validation::new(JwtAlgorithm::RS256);
        validation.set_audience(&["test_audience"]);

        let decoded = verifier
            .decode::<JwtTokenClaims>(&token.access_token, &validation)
            .unwrap();
        assert_eq!(decoded.claims.sub, "test_subject");
    }

    #[test]
    fn test_verifier_invalid_token_fails() {
        let (public_pem, _private_pem) = Jwt::dummy_keys();
        let verifier = JwtVerifier::from_rsa_public_key(&public_pem).unwrap();

        let result = verifier
            .decode::<JwtTokenClaims>("invalid.token.here", &Validation::new(JwtAlgorithm::RS256));
        assert!(result.is_err());
    }

    #[test]
    fn test_verifier_wrong_key_fails() {
        let (public_pem_1, private_pem_1) = Jwt::dummy_keys();
        let (public_pem_2, _private_pem_2) = Jwt::dummy_keys();

        let signer = Jwt::from_rsa_pem(&public_pem_1, &private_pem_1, 60).unwrap();
        let token = signer.generate(sample_claims()).unwrap();

        let verifier = JwtVerifier::from_rsa_public_key(&public_pem_2).unwrap();
        let mut validation = Validation::new(JwtAlgorithm::RS256);
        validation.set_audience(&["test_audience"]);

        let result = verifier.decode::<JwtTokenClaims>(&token.access_token, &validation);
        assert!(result.is_err());
    }

    #[test]
    fn test_verifier_from_hmac_with_algorithm() {
        let secret = b"another-secret";
        let verifier = JwtVerifier::from_hmac_with_algorithm(secret, JwtAlgorithm::HS384).unwrap();
        assert_eq!(verifier.algorithm(), JwtAlgorithm::HS384);

        // Invalid algorithm rejected
        let result = JwtVerifier::from_hmac_with_algorithm(secret, JwtAlgorithm::RS256);
        assert!(result.is_err());
    }

    #[test]
    fn test_verifier_clone() {
        let (public_pem, private_pem) = Jwt::dummy_keys();
        let signer = Jwt::from_rsa_pem(&public_pem, &private_pem, 60).unwrap();
        let token = signer.generate(sample_claims()).unwrap();

        let verifier1 = JwtVerifier::from_rsa_public_key(&public_pem).unwrap();
        let verifier2 = verifier1.clone();

        let mut validation = Validation::new(JwtAlgorithm::RS256);
        validation.set_audience(&["test_audience"]);

        let decoded = verifier2
            .decode::<JwtTokenClaims>(&token.access_token, &validation)
            .unwrap();
        assert_eq!(decoded.claims.sub, "test_subject");
    }
}

// Combined JWT + JWE tests

#[cfg(all(test, feature = "jwe"))]
mod jwe_combined_tests {
    use super::*;
    use crate::helpers::jwe::Jwe;

    fn jwe_key() -> Vec<u8> {
        b"0123456789abcdef0123456789abcdef".to_vec()
    }

    fn jwe_with_defaults() -> Jwe {
        Jwe::from_symmetric(&jwe_key()).unwrap()
    }

    #[test]
    fn test_generate_encrypted_round_trip() {
        let jwt = Jwt::from_hmac(b"jwt-secret", 60);
        let jwe = jwe_with_defaults();
        let claims = JwtTokenClaims {
            sub: "user-42".into(),
            iat: 0,
            exp: 9999999999,
            iss: "test".into(),
            aud: "app".into(),
            jti: "id-1".into(),
        };

        // sign + encrypt
        let jwe_token = jwt.generate_encrypted(claims, &jwe).unwrap();
        // JWE compact serialization has 5 parts
        assert_eq!(jwe_token.split('.').count(), 5);

        // decrypt + verify
        let mut validation = Validation::new(JwtAlgorithm::HS256);
        validation.set_audience(&["app"]);

        let decoded = jwt
            .decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe, &validation)
            .unwrap();
        assert_eq!(decoded.claims.sub, "user-42");
        assert_eq!(decoded.claims.iss, "test");
    }

    #[test]
    fn test_generate_encrypted_with_algorithm() {
        let jwt = Jwt::from_hmac(b"jwt-secret", 60);
        let jwe = jwe_with_defaults();
        let claims = JwtTokenClaims {
            sub: "user-99".into(),
            iat: 0,
            exp: 9999999999,
            iss: "test".into(),
            aud: "app".into(),
            jti: "id-2".into(),
        };

        let jwe_token = jwt
            .generate_encrypted_with_algorithm(claims, JwtAlgorithm::HS512, &jwe)
            .unwrap();

        let mut validation = Validation::new(JwtAlgorithm::HS512);
        validation.set_audience(&["app"]);

        let decoded = jwt
            .decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe, &validation)
            .unwrap();
        assert_eq!(decoded.claims.sub, "user-99");
    }

    #[test]
    fn test_decode_decrypted_wrong_jwe_key_fails() {
        let jwt = Jwt::from_hmac(b"jwt-secret", 60);
        let jwe_encrypt = jwe_with_defaults();
        let jwe_decrypt = Jwe::from_symmetric(b"wrong-key-0123456789abcdef01234567").unwrap();

        let claims = JwtTokenClaims {
            sub: "user".into(),
            iat: 0,
            exp: 9999999999,
            iss: "test".into(),
            aud: "app".into(),
            jti: "id".into(),
        };

        let jwe_token = jwt.generate_encrypted(claims, &jwe_encrypt).unwrap();

        let validation = Validation::new(JwtAlgorithm::HS256);
        let result = jwt.decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe_decrypt, &validation);
        assert!(result.is_err());
    }
}
