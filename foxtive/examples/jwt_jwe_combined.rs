//! Example: JWT algorithm configuration and combined JWT + JWE workflow.
//!
//! Run with:
//! ```sh
//! cargo run --example jwt_jwe_combined --features "jwt,jwe,test-utils"
//! ```

use foxtive::helpers::jwe::{Jwe, JweAlgorithm, JweConfig, JweEncryption};
use foxtive::helpers::jwt::{Jwt, JwtAlgorithm, JwtConfig, JwtTokenClaims};
use jsonwebtoken::Validation;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> usize {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize
}

fn make_claims(sub: &str) -> JwtTokenClaims {
    JwtTokenClaims {
        sub: sub.into(),
        iat: now_secs(),
        exp: now_secs() + 3600,
        iss: "foxtive-example".into(),
        aud: "example-app".into(),
        jti: uuid::Uuid::new_v4().to_string(),
    }
}

fn main() {
    println!("=== JWT Algorithm Config & JWT+JWE Combined Example ===\n");

    // RSA with explicit algorithm (RS512)
    println!("--- 1. RSA with RS512 algorithm ---");

    let (public_pem, private_pem) = Jwt::dummy_keys();
    let config =
        JwtConfig::rsa_pem_with_algorithm(&public_pem, &private_pem, JwtAlgorithm::RS512, 30)
            .expect("valid RSA config");

    println!("Algorithm: {:?}", config.algorithm());
    println!("Token lifetime: {} min", config.token_lifetime());

    let jwt_rsa = Jwt::new(config);
    let claims = make_claims("user-rsa-512");
    let token = jwt_rsa.generate(claims).expect("sign with RS512");
    println!("RS512 JWT: {}...\n", &token.access_token[..60]);

    let mut validation = Validation::new(JwtAlgorithm::RS512);
    validation.set_audience(&["example-app"]);
    let decoded = jwt_rsa
        .decode::<JwtTokenClaims>(&token.access_token, &validation)
        .expect("verify RS512");
    assert_eq!(decoded.claims.sub, "user-rsa-512");
    println!("✓ RS512 round-trip successful!\n");

    // HMAC with builder override via with_algorithm()
    println!("--- 2. HMAC with with_algorithm() builder ---");

    let config = JwtConfig::hmac(b"my-shared-secret", 60).with_algorithm(JwtAlgorithm::HS384);
    println!("Algorithm: {:?}", config.algorithm());

    let jwt_hmac = Jwt::new(config);
    let claims = make_claims("user-hmac-384");
    let token = jwt_hmac.generate(claims).expect("sign with HS384");

    let mut validation = Validation::new(JwtAlgorithm::HS384);
    validation.set_audience(&["example-app"]);
    let decoded = jwt_hmac
        .decode::<JwtTokenClaims>(&token.access_token, &validation)
        .expect("verify HS384");
    assert_eq!(decoded.claims.sub, "user-hmac-384");
    println!("✓ HS384 builder override successful!\n");

    // Combined JWT + JWE: sign-then-encrypt
    println!("--- 3. Combined JWT + JWE: sign then encrypt ---");

    // Set up JWT (HMAC HS256)
    let jwt = Jwt::from_hmac(b"jwt-signing-secret", 45);

    // Set up JWE (symmetric A256KW + A256GCM)
    let jwe_key = b"0123456789abcdef0123456789abcdef"; // 32 bytes
    let jwe_config = JweConfig::symmetric(jwe_key)
        .expect("valid JWE key")
        .with_defaults(JweAlgorithm::A256KW, JweEncryption::A256GCM);
    let jwe = Jwe::new(jwe_config);

    let claims = make_claims("user-42");
    println!("Claims subject: {}", claims.sub);

    // Sign JWT, then encrypt the JWT string with JWE
    let jwe_token = jwt
        .generate_encrypted(claims, &jwe)
        .expect("sign + encrypt");

    println!("JWE token (5 parts): {}...", &jwe_token[..60]);
    assert_eq!(jwe_token.split('.').count(), 5);
    println!("✓ JWT signed and encrypted into JWE envelope!\n");

    // Combined JWT + JWE: decrypt-then-verify
    println!("--- 4. Combined JWT + JWE: decrypt then verify ---");

    let mut validation = Validation::new(JwtAlgorithm::HS256);
    validation.set_audience(&["example-app"]);

    let decoded = jwt
        .decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe, &validation)
        .expect("decrypt + verify");

    assert_eq!(decoded.claims.sub, "user-42");
    assert_eq!(decoded.claims.iss, "foxtive-example");
    println!("Decrypted subject: {}", decoded.claims.sub);
    println!("Decrypted issuer: {}", decoded.claims.iss);
    println!("✓ JWE decrypted and JWT verified successfully!\n");

    // Combined with algorithm override
    println!("--- 5. Sign with HS512 + encrypt with JWE ---");

    let claims = make_claims("user-override");
    let jwe_token = jwt
        .generate_encrypted_with_algorithm(claims, JwtAlgorithm::HS512, &jwe)
        .expect("sign HS512 + encrypt");

    let mut validation = Validation::new(JwtAlgorithm::HS512);
    validation.set_audience(&["example-app"]);

    let decoded = jwt
        .decode_decrypted::<JwtTokenClaims>(&jwe_token, &jwe, &validation)
        .expect("decrypt + verify HS512");

    assert_eq!(decoded.claims.sub, "user-override");
    println!("✓ HS512 + JWE combined round-trip successful!\n");

    println!("=== All examples completed successfully! ===");
}
