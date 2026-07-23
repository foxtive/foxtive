//! Integration tests for the combined JWT + JWE API (sign-then-encrypt / decrypt-then-verify).
#![cfg(all(feature = "jwt", feature = "jwe"))]

use foxtive::helpers::jwe::{Jwe, JweAlgorithm, JweConfig, JweEncryption};
use foxtive::helpers::jwt::{Jwt, JwtConfig, JwtTokenClaims};
use foxtive::helpers::jwt::{JwtAlgorithm as Algorithm, Validation};
use std::time::{SystemTime, UNIX_EPOCH};

fn sample_claims(sub: &str, aud: &str) -> JwtTokenClaims {
    JwtTokenClaims {
        sub: sub.into(),
        iat: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize,
        exp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize
            + 3600,
        iss: "jwt-jwe-test".into(),
        aud: aud.into(),
        jti: "combined-1".into(),
    }
}

fn jwe_key() -> Vec<u8> {
    b"0123456789abcdef0123456789abcdef".to_vec()
}

fn jwe_with_defaults() -> Jwe {
    Jwe::from_symmetric(&jwe_key()).unwrap()
}

// HMAC JWT + JWE

#[test]
fn test_hmac_jwt_generate_encrypted_round_trip() {
    let jwt = Jwt::from_hmac(b"hmac-secret", 60);
    let jwe = jwe_with_defaults();
    let claims = sample_claims("alice", "my-app");

    let jwe_token = jwt.generate_encrypted(claims, &jwe).unwrap();

    // JWE compact serialization has 5 dot-separated parts
    assert_eq!(jwe_token.split('.').count(), 5);

    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["my-app"]);

    let decoded = jwt
        .decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe, &validation)
        .unwrap();
    assert_eq!(decoded.claims.sub, "alice");
    assert_eq!(decoded.claims.iss, "jwt-jwe-test");
}

#[test]
fn test_hmac_jwt_with_custom_algorithm() {
    let config = JwtConfig::hmac_with_algorithm(b"hmac-384-secret", Algorithm::HS384, 30).unwrap();
    let jwt = Jwt::new(config);
    let jwe = jwe_with_defaults();
    let claims = sample_claims("bob", "secure-app");

    let jwe_token = jwt.generate_encrypted(claims, &jwe).unwrap();

    let mut validation = Validation::new(Algorithm::HS384);
    validation.set_audience(&["secure-app"]);

    let decoded = jwt
        .decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe, &validation)
        .unwrap();
    assert_eq!(decoded.claims.sub, "bob");
}

#[test]
fn test_generate_encrypted_with_algorithm_override() {
    let jwt = Jwt::from_hmac(b"default-hs256", 60);
    let jwe = jwe_with_defaults();
    let claims = sample_claims("charlie", "override-app");

    // Override to HS512 at generation time
    let jwe_token = jwt
        .generate_encrypted_with_algorithm(claims, Algorithm::HS512, &jwe)
        .unwrap();

    let mut validation = Validation::new(Algorithm::HS512);
    validation.set_audience(&["override-app"]);

    let decoded = jwt
        .decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe, &validation)
        .unwrap();
    assert_eq!(decoded.claims.sub, "charlie");
}

// RSA JWT + JWE

#[test]
fn test_rsa_jwt_generate_encrypted_round_trip() {
    let (pub_pem, priv_pem) = Jwt::dummy_keys();
    let jwt = Jwt::from_rsa_pem(&pub_pem, &priv_pem, 60).unwrap();
    let jwe = jwe_with_defaults();
    let claims = sample_claims("rsa-user", "rsa-app");

    let jwe_token = jwt.generate_encrypted(claims, &jwe).unwrap();
    assert_eq!(jwe_token.split('.').count(), 5);

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&["rsa-app"]);

    let decoded = jwt
        .decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe, &validation)
        .unwrap();
    assert_eq!(decoded.claims.sub, "rsa-user");
}

#[test]
fn test_rsa_jwt_with_rs512_generate_encrypted() {
    let (pub_pem, priv_pem) = Jwt::dummy_keys();
    let config =
        JwtConfig::rsa_pem_with_algorithm(&pub_pem, &priv_pem, Algorithm::RS512, 45).unwrap();
    let jwt = Jwt::new(config);
    let jwe = jwe_with_defaults();
    let claims = sample_claims("rsa-512-user", "strong-app");

    let jwe_token = jwt.generate_encrypted(claims, &jwe).unwrap();

    let mut validation = Validation::new(Algorithm::RS512);
    validation.set_audience(&["strong-app"]);

    let decoded = jwt
        .decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe, &validation)
        .unwrap();
    assert_eq!(decoded.claims.sub, "rsa-512-user");
    assert_eq!(decoded.claims.iss, "jwt-jwe-test");
}

// Error cases

#[test]
fn test_decrypt_with_wrong_jwe_key_fails() {
    let jwt = Jwt::from_hmac(b"jwt-secret", 60);
    let jwe_encrypt = jwe_with_defaults();
    let jwe_decrypt = Jwe::from_symmetric(b"wrong-key-0123456789abcdef01234567").unwrap();

    let claims = sample_claims("user", "app");
    let jwe_token = jwt.generate_encrypted(claims, &jwe_encrypt).unwrap();

    let validation = Validation::new(Algorithm::HS256);
    let result = jwt.decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe_decrypt, &validation);
    assert!(result.is_err());
}

#[test]
fn test_decrypt_with_wrong_jwt_secret_fails() {
    let jwt_encrypt = Jwt::from_hmac(b"correct-secret", 60);
    let jwt_decrypt = Jwt::from_hmac(b"wrong-secret", 60);
    let jwe = jwe_with_defaults();

    let claims = sample_claims("user", "app");
    let jwe_token = jwt_encrypt.generate_encrypted(claims, &jwe).unwrap();

    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["app"]);

    let result = jwt_decrypt.decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe, &validation);
    assert!(result.is_err());
}

#[test]
fn test_decrypt_with_wrong_audience_fails() {
    let jwt = Jwt::from_hmac(b"secret", 60);
    let jwe = jwe_with_defaults();
    let claims = sample_claims("user", "correct-aud");

    let jwe_token = jwt.generate_encrypted(claims, &jwe).unwrap();

    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["wrong-aud"]);

    let result = jwt.decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe, &validation);
    assert!(result.is_err());
}

#[test]
fn test_decrypt_with_wrong_algorithm_validation_fails() {
    let jwt = Jwt::from_hmac(b"secret", 60);
    let jwe = jwe_with_defaults();
    let claims = sample_claims("user", "app");

    let jwe_token = jwt.generate_encrypted(claims, &jwe).unwrap();

    // Token was signed with HS256, but we try to validate with HS512
    let validation = Validation::new(Algorithm::HS512);
    let result = jwt.decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe, &validation);
    assert!(result.is_err());
}

// JWE defaults are always available

#[test]
fn test_generate_encrypted_with_default_jwe() {
    let jwt = Jwt::from_hmac(b"secret", 60);
    let jwe = Jwe::from_symmetric(&jwe_key()).unwrap();
    let claims = sample_claims("user", "app");

    let jwe_token = jwt.generate_encrypted(claims, &jwe).unwrap();
    assert_eq!(jwe_token.split('.').count(), 5);

    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["app"]);

    let decoded = jwt
        .decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe, &validation)
        .unwrap();
    assert_eq!(decoded.claims.sub, "user");
}

// Clone preserves combined behavior

#[test]
fn test_cloned_jwt_can_decrypt_encrypted_token() {
    let jwt = Jwt::from_hmac(b"clone-secret", 60);
    let jwt_clone = jwt.clone();
    let jwe = jwe_with_defaults();

    let claims = sample_claims("clone-user", "clone-app");
    let jwe_token = jwt.generate_encrypted(claims, &jwe).unwrap();

    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["clone-app"]);

    let decoded = jwt_clone
        .decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe, &validation)
        .unwrap();
    assert_eq!(decoded.claims.sub, "clone-user");
}

// Different JWE encryption algorithms

#[test]
fn test_jwt_encrypted_with_a128kw() {
    let jwt = Jwt::from_hmac(b"jwt-secret", 60);
    let key_128 = b"0123456789abcdef"; // 16 bytes for A128KW
    let jwe_config = JweConfig::symmetric(key_128)
        .unwrap()
        .with_defaults(JweAlgorithm::A128KW, JweEncryption::A128GCM);
    let jwe = Jwe::new(jwe_config);

    let claims = sample_claims("a128-user", "a128-app");
    let jwe_token = jwt.generate_encrypted(claims, &jwe).unwrap();

    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["a128-app"]);

    let decoded = jwt
        .decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe, &validation)
        .unwrap();
    assert_eq!(decoded.claims.sub, "a128-user");
}

#[test]
fn test_jwt_encrypted_with_cbc_encryption() {
    let jwt = Jwt::from_hmac(b"jwt-secret", 60);
    let jwe_config = JweConfig::symmetric(&jwe_key())
        .unwrap()
        .with_defaults(JweAlgorithm::A256KW, JweEncryption::A256CbcHs512);
    let jwe = Jwe::new(jwe_config);

    let claims = sample_claims("cbc-user", "cbc-app");
    let jwe_token = jwt.generate_encrypted(claims, &jwe).unwrap();

    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["cbc-app"]);

    let decoded = jwt
        .decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe, &validation)
        .unwrap();
    assert_eq!(decoded.claims.sub, "cbc-user");
}
