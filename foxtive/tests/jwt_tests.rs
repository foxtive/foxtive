//! Integration tests for JWT algorithm configuration.
#![cfg(feature = "jwt")]

use foxtive::helpers::jwt::{Jwt, JwtConfig, JwtTokenClaims};
use foxtive::helpers::jwt::{JwtAlgorithm as Algorithm, Validation};
use std::time::{SystemTime, UNIX_EPOCH};

fn sample_claims() -> JwtTokenClaims {
    JwtTokenClaims {
        sub: "integration-user".into(),
        iat: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize,
        exp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize
            + 3600,
        iss: "integration-test".into(),
        aud: "test-suite".into(),
        jti: "int-test-1".into(),
    }
}

#[test]
fn test_rsa_pem_default_algorithm_is_rs256() {
    let (pub_pem, priv_pem) = Jwt::dummy_keys();
    let config = JwtConfig::rsa_pem(&pub_pem, &priv_pem, 60).unwrap();
    assert_eq!(config.algorithm(), Algorithm::RS256);
}

#[test]
fn test_rsa_pem_with_algorithm_rs384() {
    let (pub_pem, priv_pem) = Jwt::dummy_keys();
    let config =
        JwtConfig::rsa_pem_with_algorithm(&pub_pem, &priv_pem, Algorithm::RS384, 45).unwrap();

    assert_eq!(config.algorithm(), Algorithm::RS384);
    assert_eq!(config.token_lifetime(), 45);

    let jwt = Jwt::new(config);
    let token = jwt.generate(sample_claims()).unwrap();

    let mut validation = Validation::new(Algorithm::RS384);
    validation.set_audience(&["test-suite"]);

    let decoded = jwt
        .decode::<JwtTokenClaims>(&token.access_token, &validation)
        .unwrap();
    assert_eq!(decoded.claims.sub, "integration-user");
    assert_eq!(decoded.claims.iss, "integration-test");
}

#[test]
fn test_rsa_pem_with_algorithm_rs512() {
    let (pub_pem, priv_pem) = Jwt::dummy_keys();
    let config =
        JwtConfig::rsa_pem_with_algorithm(&pub_pem, &priv_pem, Algorithm::RS512, 90).unwrap();

    assert_eq!(config.algorithm(), Algorithm::RS512);

    let jwt = Jwt::new(config);
    let token = jwt.generate(sample_claims()).unwrap();

    let mut validation = Validation::new(Algorithm::RS512);
    validation.set_audience(&["test-suite"]);

    let decoded = jwt
        .decode::<JwtTokenClaims>(&token.access_token, &validation)
        .unwrap();
    assert_eq!(decoded.claims.aud, "test-suite");
}

#[test]
fn test_rsa_pem_with_non_rsa_algorithm_rejected() {
    let (pub_pem, priv_pem) = Jwt::dummy_keys();

    // HMAC algorithms should be rejected for RSA keys
    for alg in [Algorithm::HS256, Algorithm::HS384, Algorithm::HS512] {
        let result = JwtConfig::rsa_pem_with_algorithm(&pub_pem, &priv_pem, alg, 60);
        assert!(result.is_err(), "Expected error for {:?}", alg);
    }
}

#[test]
fn test_hmac_default_algorithm_is_hs256() {
    let config = JwtConfig::hmac(b"secret", 60);
    assert_eq!(config.algorithm(), Algorithm::HS256);
}

#[test]
fn test_hmac_with_algorithm_hs384() {
    let config = JwtConfig::hmac_with_algorithm(b"secret", Algorithm::HS384, 30).unwrap();
    assert_eq!(config.algorithm(), Algorithm::HS384);

    let jwt = Jwt::new(config);
    let token = jwt.generate(sample_claims()).unwrap();

    let mut validation = Validation::new(Algorithm::HS384);
    validation.set_audience(&["test-suite"]);

    let decoded = jwt
        .decode::<JwtTokenClaims>(&token.access_token, &validation)
        .unwrap();
    assert_eq!(decoded.claims.sub, "integration-user");
}

#[test]
fn test_hmac_with_algorithm_hs512() {
    let config = JwtConfig::hmac_with_algorithm(b"secret", Algorithm::HS512, 120).unwrap();
    assert_eq!(config.algorithm(), Algorithm::HS512);
    assert_eq!(config.token_lifetime(), 120);
}

#[test]
fn test_hmac_with_non_hmac_algorithm_rejected() {
    for alg in [Algorithm::RS256, Algorithm::RS384, Algorithm::RS512] {
        let result = JwtConfig::hmac_with_algorithm(b"secret", alg, 60);
        assert!(result.is_err(), "Expected error for {:?}", alg);
    }
}

#[test]
fn test_with_algorithm_overrides_hmac_default() {
    let config = JwtConfig::hmac(b"secret", 60).with_algorithm(Algorithm::HS512);
    assert_eq!(config.algorithm(), Algorithm::HS512);

    let jwt = Jwt::new(config);
    let token = jwt.generate(sample_claims()).unwrap();

    let mut validation = Validation::new(Algorithm::HS512);
    validation.set_audience(&["test-suite"]);

    let decoded = jwt
        .decode::<JwtTokenClaims>(&token.access_token, &validation)
        .unwrap();
    assert_eq!(decoded.claims.sub, "integration-user");
}

#[test]
fn test_with_algorithm_overrides_rsa_default() {
    let (pub_pem, priv_pem) = Jwt::dummy_keys();
    let config = JwtConfig::rsa_pem(&pub_pem, &priv_pem, 60)
        .unwrap()
        .with_algorithm(Algorithm::RS384);
    assert_eq!(config.algorithm(), Algorithm::RS384);

    let jwt = Jwt::new(config);
    let token = jwt.generate(sample_claims()).unwrap();

    let mut validation = Validation::new(Algorithm::RS384);
    validation.set_audience(&["test-suite"]);

    let decoded = jwt
        .decode::<JwtTokenClaims>(&token.access_token, &validation)
        .unwrap();
    assert_eq!(decoded.claims.iss, "integration-test");
}

#[test]
fn test_config_mut_algorithm_change() {
    let mut jwt = Jwt::from_hmac(b"mutable-secret", 60);
    assert_eq!(jwt.config().algorithm(), Algorithm::HS256);

    // Change algorithm via mutable config reference
    *jwt.config_mut() = jwt.config().clone().with_algorithm(Algorithm::HS512);
    assert_eq!(jwt.config().algorithm(), Algorithm::HS512);

    let token = jwt.generate(sample_claims()).unwrap();

    let mut validation = Validation::new(Algorithm::HS512);
    validation.set_audience(&["test-suite"]);

    let decoded = jwt
        .decode::<JwtTokenClaims>(&token.access_token, &validation)
        .unwrap();
    assert_eq!(decoded.claims.sub, "integration-user");
}

#[test]
fn test_generate_with_algorithm_overrides_config_default() {
    let jwt = Jwt::from_hmac(b"secret", 60);
    assert_eq!(jwt.config().algorithm(), Algorithm::HS256);

    // Override at generation time
    let token = jwt
        .generate_with_algorithm(sample_claims(), Algorithm::HS384)
        .unwrap();

    let mut validation = Validation::new(Algorithm::HS384);
    validation.set_audience(&["test-suite"]);

    let decoded = jwt
        .decode::<JwtTokenClaims>(&token.access_token, &validation)
        .unwrap();
    assert_eq!(decoded.claims.sub, "integration-user");
}

#[test]
fn test_decode_hs384_token_with_hs256_validation_fails() {
    let config = JwtConfig::hmac_with_algorithm(b"shared", Algorithm::HS384, 60).unwrap();
    let jwt = Jwt::new(config);

    let token = jwt.generate(sample_claims()).unwrap();

    // Try to validate with wrong algorithm
    let result =
        jwt.decode::<JwtTokenClaims>(&token.access_token, &Validation::new(Algorithm::HS256));
    assert!(result.is_err());
}

#[test]
fn test_decode_rs384_token_with_rs256_validation_fails() {
    let (pub_pem, priv_pem) = Jwt::dummy_keys();
    let config =
        JwtConfig::rsa_pem_with_algorithm(&pub_pem, &priv_pem, Algorithm::RS384, 60).unwrap();
    let jwt = Jwt::new(config);

    let token = jwt.generate(sample_claims()).unwrap();

    let result =
        jwt.decode::<JwtTokenClaims>(&token.access_token, &Validation::new(Algorithm::RS256));
    assert!(result.is_err());
}

// ─────────────────────────────────────────────────────────────────────────────
// Clone preserves algorithm
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_jwt_clone_preserves_algorithm() {
    let config = JwtConfig::hmac(b"secret", 60).with_algorithm(Algorithm::HS512);
    let jwt1 = Jwt::new(config);
    let jwt2 = jwt1.clone();

    assert_eq!(jwt2.config().algorithm(), Algorithm::HS512);

    let token = jwt1.generate(sample_claims()).unwrap();

    let mut validation = Validation::new(Algorithm::HS512);
    validation.set_audience(&["test-suite"]);

    let decoded = jwt2
        .decode::<JwtTokenClaims>(&token.access_token, &validation)
        .unwrap();
    assert_eq!(decoded.claims.sub, "integration-user");
}
