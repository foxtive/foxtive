//! Integration tests for the JWE module.
#![cfg(feature = "jwe")]

use foxtive::helpers::jwe::{Jwe, JweAlgorithm, JweEncryption};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct UserToken {
    user_id: u64,
    username: String,
    roles: Vec<String>,
}

fn test_user() -> UserToken {
    UserToken {
        user_id: 12345,
        username: "alice".into(),
        roles: vec!["admin".into(), "editor".into()],
    }
}

fn symmetric_key() -> Vec<u8> {
    // 32 bytes for A256KW
    b"0123456789abcdef0123456789abcdef".to_vec()
}

// Symmetric key tests

#[test]
fn test_symmetric_a256kw_a256gcm_round_trip() {
    let jwe = Jwe::from_symmetric(&symmetric_key()).unwrap();
    let user = test_user();

    let token = jwe
        .encrypt_with(&user, JweAlgorithm::A256KW, JweEncryption::A256GCM)
        .unwrap();

    // Compact serialization: 5 dot-separated parts
    assert_eq!(token.split('.').count(), 5);

    let decrypted: UserToken = jwe.decrypt(&token).unwrap();
    assert_eq!(decrypted, user);
}

#[test]
fn test_symmetric_a128kw_a128gcm_round_trip() {
    let key = b"0123456789abcdef"; // 16 bytes for A128KW
    let jwe = Jwe::from_symmetric(key).unwrap();
    let user = test_user();

    let token = jwe
        .encrypt_with(&user, JweAlgorithm::A128KW, JweEncryption::A128GCM)
        .unwrap();

    let decrypted: UserToken = jwe.decrypt(&token).unwrap();
    assert_eq!(decrypted, user);
}

#[test]
fn test_symmetric_a192kw_a192gcm_round_trip() {
    let key = b"0123456789abcdef01234567"; // 24 bytes for A192KW
    let jwe = Jwe::from_symmetric(key).unwrap();
    let user = test_user();

    let token = jwe
        .encrypt_with(&user, JweAlgorithm::A192KW, JweEncryption::A192GCM)
        .unwrap();

    let decrypted: UserToken = jwe.decrypt(&token).unwrap();
    assert_eq!(decrypted, user);
}

#[test]
fn test_symmetric_dir_a256gcm() {
    // Dir uses the key directly as CEK - must match enc algorithm's key size
    let key = b"0123456789abcdef0123456789abcdef"; // 32 bytes for A256GCM
    let jwe = Jwe::from_symmetric(key).unwrap();
    let user = test_user();

    let token = jwe
        .encrypt_with(&user, JweAlgorithm::Dir, JweEncryption::A256GCM)
        .unwrap();

    let decrypted: UserToken = jwe.decrypt(&token).unwrap();
    assert_eq!(decrypted, user);
}

#[test]
fn test_symmetric_a256kw_cbc_hs512() {
    let jwe = Jwe::from_symmetric(&symmetric_key()).unwrap();
    let user = test_user();

    let token = jwe
        .encrypt_with(&user, JweAlgorithm::A256KW, JweEncryption::A256CbcHs512)
        .unwrap();

    let decrypted: UserToken = jwe.decrypt(&token).unwrap();
    assert_eq!(decrypted, user);
}

// RSA tests

#[test]
fn test_rsa_oaep_256_round_trip() {
    use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey};
    use rsa::{RsaPrivateKey, RsaPublicKey};

    let mut rng = rand::thread_rng();
    let private_key = RsaPrivateKey::new(&mut rng, 2048).unwrap();
    let public_key = RsaPublicKey::from(&private_key);

    let spki_der = public_key.to_public_key_der().unwrap().as_ref().to_vec();
    let pkcs8_der = private_key.to_pkcs8_der().unwrap().as_bytes().to_vec();

    let encrypter = Jwe::from_symmetric(&spki_der).unwrap();
    let decrypter = Jwe::from_symmetric(&pkcs8_der).unwrap();

    let user = test_user();
    let token = encrypter
        .encrypt_with(&user, JweAlgorithm::RsaOaep256, JweEncryption::A256GCM)
        .unwrap();

    let decrypted: UserToken = decrypter.decrypt(&token).unwrap();
    assert_eq!(decrypted, user);
}

// Raw bytes tests

#[test]
fn test_raw_bytes_round_trip() {
    let jwe = Jwe::from_symmetric(&symmetric_key()).unwrap();
    let plaintext = b"binary data: \x00\x01\x02\xff\xfe";

    let token = jwe
        .encrypt_raw_with(plaintext, JweAlgorithm::A256KW, JweEncryption::A256GCM)
        .unwrap();

    let decrypted = jwe.decrypt_raw(&token).unwrap();
    assert_eq!(decrypted, plaintext);
}

// Error cases

#[test]
fn test_empty_key_rejected() {
    let result = Jwe::from_symmetric(b"");
    assert!(result.is_err());
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
fn test_decrypt_tampered_token_fails() {
    let jwe = Jwe::from_symmetric(&symmetric_key()).unwrap();
    let user = test_user();

    let token = jwe
        .encrypt_with(&user, JweAlgorithm::A256KW, JweEncryption::A256GCM)
        .unwrap();

    // Tamper with the ciphertext (4th part)
    let parts: Vec<&str> = token.split('.').collect();
    let mut tampered: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
    if let Some(ciphertext) = tampered.get_mut(3) {
        let mut chars: Vec<char> = ciphertext.chars().collect();
        if !chars.is_empty() {
            chars[0] = if chars[0] == 'A' { 'B' } else { 'A' };
            *ciphertext = chars.into_iter().collect();
        }
    }
    let tampered_token = tampered.join(".");

    let result: foxtive::prelude::AppResult<UserToken> = jwe.decrypt(&tampered_token);
    assert!(result.is_err());
}

// Clone test

#[test]
fn test_jwe_clone() {
    let jwe1 = Jwe::from_symmetric(&symmetric_key()).unwrap();
    let jwe2 = jwe1.clone();

    let token = jwe1
        .encrypt_raw_with(b"test", JweAlgorithm::A256KW, JweEncryption::A256GCM)
        .unwrap();

    // Can decrypt with cloned instance
    let decrypted = jwe2.decrypt_raw(&token).unwrap();
    assert_eq!(decrypted, b"test");
}
