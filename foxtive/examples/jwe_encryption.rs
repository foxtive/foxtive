//! Example: JWE encryption and decryption.
//!
//! Run with:
//! ```sh
//! cargo run --example jwe_encryption --features jwe
//! ```

use foxtive::helpers::jwe::{Jwe, JweAlgorithm, JweConfig, JweEncryption};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct SensitiveData {
    ssn: String,
    credit_card: String,
    notes: String,
}

fn main() {
    println!("=== JWE Encryption Example ===\n");

    // Symmetric key encryption (A256KW + A256GCM)
    println!("--- Symmetric Key Encryption (A256KW + A256GCM) ---");

    // 32-byte key for A256KW
    let key = b"0123456789abcdef0123456789abcdef";
    let config = JweConfig::symmetric(key)
        .expect("valid key")
        .with_defaults(JweAlgorithm::A256KW, JweEncryption::A256GCM);
    let jwe = Jwe::new(config);

    let data = SensitiveData {
        ssn: "123-45-6789".into(),
        credit_card: "4111-1111-1111-1111".into(),
        notes: "Top secret customer data".into(),
    };

    println!("Plaintext: {:?}", data);

    let token = jwe.encrypt(&data).expect("encrypt");

    println!("JWE token: {}...\n", &token[..60]);

    let decrypted: SensitiveData = jwe.decrypt(&token).expect("decrypt");
    println!("Decrypted: {:?}", decrypted);
    assert_eq!(decrypted, data);
    println!("✓ Round-trip successful!\n");

    // Direct key encryption (Dir + A128GCM)
    println!("--- Direct Key Encryption (Dir + A128GCM) ---");

    // 16-byte CEK for A128GCM
    let cek = b"0123456789abcdef";
    let jwe_dir = Jwe::from_symmetric(cek).expect("valid key");

    let token = jwe_dir
        .encrypt_raw_with(b"Hello, JWE!", JweAlgorithm::Dir, JweEncryption::A128GCM)
        .expect("encrypt");

    println!("JWE token: {}...", &token[..60]);

    let plaintext = jwe_dir.decrypt_raw(&token).expect("decrypt");
    println!("Decrypted: {:?}\n", String::from_utf8_lossy(&plaintext));
    assert_eq!(plaintext, b"Hello, JWE!");
    println!("✓ Direct encryption successful!\n");

    // AES-CBC encryption (A128KW + A128CBC-HS256)
    println!("--- AES-CBC Encryption (A128KW + A128CBC-HS256) ---");

    let key_128 = b"0123456789abcdef"; // 16 bytes for A128KW
    let jwe_cbc = Jwe::from_symmetric(key_128).expect("valid key");

    let token = jwe_cbc
        .encrypt_raw_with(
            b"CBC mode payload",
            JweAlgorithm::A128KW,
            JweEncryption::A128CbcHs256,
        )
        .expect("encrypt");

    println!("JWE token: {}...", &token[..60]);

    let plaintext = jwe_cbc.decrypt_raw(&token).expect("decrypt");
    println!("Decrypted: {:?}\n", String::from_utf8_lossy(&plaintext));
    assert_eq!(plaintext, b"CBC mode payload");
    println!("✓ CBC encryption successful!\n");

    println!("=== All examples completed successfully! ===");
}
