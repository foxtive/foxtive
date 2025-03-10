use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, TokenData};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub use jsonwebtoken::{Algorithm, Validation};

use crate::prelude::AppResult;

#[derive(Clone)]
pub struct Jwt {
    /// public key - will be used to verify the token
    public_key: String,
    /// private key - will be used to sign the token
    private_key: String,
    /// token lifetime (in minutes)
    token_lifetime: i64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtTokenClaims {
    /// Identifies the subject (user or entity) the token is about.
    pub sub: String,
    /// Indicates when the token was issued. Useful for token freshness. (time in timestamp)
    pub iat: usize,
    /// Specifies when the token expires. Helps prevent token reuse.
    pub exp: usize,
    /// Identifies the entity that issued the token (e.g., authentication server).
    pub iss: String,
    /// Defines the intended recipient of the token (e.g., a specific API).
    pub aud: String,
    /// A unique identifier for the token. Helps prevent replay attacks.
    pub jti: String,
}

#[derive(Serialize, Debug)]
pub struct AuthTokenData {
    /// acquired access token
    pub access_token: String,
    /// token type (typically 'bearer')
    pub token_type: String,
    /// token lifetime (in minutes)
    pub expires_in: i64,
}

impl Jwt {
    ///
    ///
    /// # Arguments
    ///
    /// * `public_key`: public key - will be used to verify the token
    /// * `private_key`: private key - will be used to sign the token
    /// * `token_lifetime`: token lifetime (in minutes)
    ///
    /// returns: Jwt
    ///
    /// # Examples
    ///
    /// ```
    /// use foxtive::helpers::jwt::{Jwt, JwtTokenClaims};
    ///
    /// let (private_key, public_key) = Jwt::dummy_keys();
    /// let jwt = Jwt::new(public_key, private_key, 60);
    /// ```
    pub fn new(public_key: String, private_key: String, token_lifetime: i64) -> Self {
        Jwt {
            public_key,
            private_key,
            token_lifetime,
        }
    }

    ///
    ///
    /// # Arguments
    ///
    /// * `claims`:
    ///
    /// returns: Result<AuthTokenData, AppMessage>
    ///
    /// # Examples
    ///
    /// ```
    /// use foxtive::helpers::jwt::{Jwt, JwtTokenClaims};
    ///
    /// let (public_key, private_key) = Jwt::dummy_keys();
    /// let jwt = Jwt::new(public_key, private_key, 60);
    ///
    /// let claims = JwtTokenClaims {
    ///     sub: "".to_string(),
    ///     iat: 0,
    ///     exp: 0,
    ///     iss: "".to_string(),
    ///     aud: "my-audience".to_string(),
    ///     jti: "abc".to_string(),
    /// };
    /// let token = jwt.generate(claims).unwrap();
    ///
    /// println!("JWT Token: {}", token.access_token);
    /// ```
    pub fn generate<C: Serialize>(&self, claims: C) -> AppResult<AuthTokenData> {
        let token_header = Header::new(Algorithm::RS256);
        let encoding_key = EncodingKey::from_rsa_pem(self.private_key.as_bytes())?;

        let token = encode(&token_header, &claims, &encoding_key)?;

        Ok(AuthTokenData {
            access_token: token,
            token_type: "bearer".to_string(),
            expires_in: self.token_lifetime,
        })
    }

    ///
    ///
    /// # Arguments
    ///
    /// * `token`:
    /// * `val`: Validation, something like `Validation::new(Algorithm::RS256)`
    ///
    /// returns: Result<TokenData<C>, AppMessage>
    ///
    /// # Examples
    ///
    /// ```
    /// use foxtive::helpers::jwt::{Jwt, Validation, JwtTokenClaims, Algorithm};
    /// use foxtive::helpers::time::current_timestamp;
    ///
    /// let (public_key, private_key) = Jwt::dummy_keys();
    /// let jwt = Jwt::new(public_key, private_key, 60);
    ///
    /// let claims = JwtTokenClaims {
    ///     sub: "".to_string(),
    ///     iat: current_timestamp() as usize,
    ///     exp: (current_timestamp() + 100) as usize,
    ///     iss: "example.com".to_string(),
    ///     aud: "my-audience".to_string(),
    ///     jti: "abc".to_string(),
    /// };
    ///
    /// let token = jwt.generate(claims).unwrap().access_token;
    ///
    /// let mut val = Validation::new(Algorithm::RS256);
    /// val.set_audience(&["my-audience"]);
    /// let claims = jwt.decode::<JwtTokenClaims>(&token, &val).unwrap();
    ///
    /// println!("Token Payload: {}", claims.claims.sub);
    /// ```
    pub fn decode<C: DeserializeOwned>(
        &self,
        token: &str,
        val: &Validation,
    ) -> AppResult<TokenData<C>> {
        Ok(decode::<C>(
            token,
            &DecodingKey::from_rsa_pem(self.public_key.as_ref())?,
            val,
        )?)
    }

    /// Returns sample keys for testing purposes.
    /// Returns a tuple of private and public keys.
    /// # Returns
    /// `(public_key, private_key)`
    pub fn dummy_keys() -> (String, String) {
        // Replace with your actual test RSA keys or mock keys
        let private_key = "-----BEGIN RSA PRIVATE KEY-----
MIIEogIBAAKCAQEA45YvzSUfRDq2wHp7kvgCn1wk6MVGyosLIf9nAutXNVA4DLDI
SAAO1C2nmZyCMX78xqkbM47WqsDLOD/WpreMz//EZffdQh5iZt0cwnBXLJvON3AO
gGtKMsTP0tb211944V2tyBVsY/Dgvs3bJfv+Q3eZXYN5HnBW1m6jmK+9i/J3gdiZ
UlfiOUxRJRz+UMwCxi++zN2FKrNJYS29SzJ3nWuZqgw4TqDlIohlS4NJj1WsLG8i
G7TM5uTxvL8W31qRT/3bWu5NPDP64+7RIx7nh2LUuGyIocvnsRYleX123ziO/Phq
iW+ieTkx3HO0Z4DJuTLqgAtKDr/+CWhE+ROQQQIDAQABAoIBAF1TWI3Ew8IMW6Wp
dGQrrJOhywbi4ukqxGlwHNNfI1da6mkV00NnNq1+fURqIB9g5hInBV8Km2/Q+GS8
FO8vxKAEz4pK7qHu6MYqtODtBWpnB9TG8ENr+7S+7UQV34oo/d5YtmsekeCXxemo
a83zPGx5LlxhDflT8uAi5ZY2tsEcajaGrKsr4bgrgZ93duLmW3M/a2mxJsk2KsUZ
kU/FVxTqkAiV8Xt7UR4K+yg4TX9UckZ5evoaHUsfwcgyH1kToe0rDNeuqaNmdA6M
bqrIBMPqxSsIHkIEhL8e3ayuUstJYFFvgcX/wjjBxN750C8DADfD5PufDRLn+6BA
to/yLysCgYEA9N9JwOsVtB/qwPabQQJ5NaeexAM4LKKjSu4pbqWk5lGAjoVFLtZA
6yQDqIIi9MD+sNzOR3eT6+unKaIQgZfqEKIuqcHK5f/UMCW7DVY6Tsfcmk63yals
9F5I7ECLvcV3DPSLMsHvuf/x2ExWCvzhHJdUeIR4XIOmmbAaZUbFAR8CgYEA7e3O
y25E3NRjfoCKsymNO9Ju1UFQB1DxLtyFWn+O3U4OJ+ygzMl+UKzZNcCYC50T7co8
mI4uIopIzwGPBhwaKnkEj8vUXqhETki+tJwImBT6xS+hKIOp1ugiymUBcg+r1zHq
8Q4kccXrFQcNf/PZnVJffjuRUioS88fclyp2Yp8CgYAlY97oJT1IJsN6uW0VbSJC
7hhRB8jRD/htHZaX/ZUDUhiWKoVY1IBPJ02jslNSGhIJwX2B7iFZGb+JnOR2VVL5
PTpw01V+7yRgQbKhI7R9iQkHStPHaCZp3ee2o1hzHq7B/Kkwk+pd9VXhWC6fOOPK
sNTGHjwerD+JIhm1syurswKBgH4QVlVrI4lt7xmDufmbtn1PzfGoHWMC/Ac8SFve
4i9XwCBfPAPFbuwH6T6VwPnGTFzdnqWmD8O1SUaFKOav1R1T6ZrXALr2pNplqMXB
Nrx9wTDhP55bxI1dibF7OvzYWNA4XqdCOwUdPKVziy+UYGT+1CHqJeFG8avA5Zwi
n385AoGAKmh3MHMfWKBiR7rUUJQa57oKU4oaU57AyhOVuV3cm7s/x4z0vdFsplmp
isI1KHomCrqIg89Ybn4n6/Ph30H7OyyMt3L7y6P+GqdnfMocvFMfQBZPagsSsDlG
69eSqwbzi08UVhDCOYqD117jm97DgK015jnom2dutUR+IUaiYWo=
-----END RSA PRIVATE KEY-----";
        let public_key = "-----BEGIN RSA PUBLIC KEY-----
MIIBCgKCAQEA45YvzSUfRDq2wHp7kvgCn1wk6MVGyosLIf9nAutXNVA4DLDISAAO
1C2nmZyCMX78xqkbM47WqsDLOD/WpreMz//EZffdQh5iZt0cwnBXLJvON3AOgGtK
MsTP0tb211944V2tyBVsY/Dgvs3bJfv+Q3eZXYN5HnBW1m6jmK+9i/J3gdiZUlfi
OUxRJRz+UMwCxi++zN2FKrNJYS29SzJ3nWuZqgw4TqDlIohlS4NJj1WsLG8iG7TM
5uTxvL8W31qRT/3bWu5NPDP64+7RIx7nh2LUuGyIocvnsRYleX123ziO/PhqiW+i
eTkx3HO0Z4DJuTLqgAtKDr/+CWhE+ROQQQIDAQAB
-----END RSA PUBLIC KEY-----";
        (public_key.to_string(), private_key.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::errors::{Error, ErrorKind};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn get_sample_claim() -> JwtTokenClaims {
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

    #[test]
    fn test_jwt_new() {
        let (public_key, private_key) = Jwt::dummy_keys();
        let jwt = Jwt::new(public_key.clone(), private_key.clone(), 60);

        assert_eq!(jwt.public_key, public_key);
        assert_eq!(jwt.private_key, private_key);
        assert_eq!(jwt.token_lifetime, 60);
    }

    #[test]
    fn test_jwt_generate() {
        let (public_key, private_key) = Jwt::dummy_keys();
        println!(">>>>>>>>>>>>>>>>>>>>>>>>>>>>> 1");
        let jwt = Jwt::new(public_key, private_key, 60);

        println!(">>>>>>>>>>>>>>>>>>>>>>>>>>>>> 2");
        let claims = get_sample_claim();

        println!(">>>>>>>>>>>>>>>>>>>>>>>>>>>>> 3");
        let result = jwt.generate(claims);

        println!(">>>>>>>>>>>>>>>>>>>>>>>>>>>>> 4");

        assert!(result.is_ok());

        let auth_token_data = result.unwrap();
        assert_eq!(auth_token_data.token_type, "bearer");
        assert_eq!(auth_token_data.expires_in, 60);
        assert!(!auth_token_data.access_token.is_empty());
    }

    #[test]
    fn test_jwt_decode() {
        let (public_key, private_key) = Jwt::dummy_keys();
        println!(">>>>>>>>>>>>>>>>>>>>>>>>>>>>> 1");
        let jwt = Jwt::new(public_key.clone(), private_key, 60);
        println!(">>>>>>>>>>>>>>>>>>>>>>>>>>>>> 2");

        let claims = get_sample_claim();
        println!(">>>>>>>>>>>>>>>>>>>>>>>>>>>>> 3");

        let generated_token = jwt.generate(claims.clone()).unwrap();
        println!(">>>>>>>>>>>>>>>>>>>>>>>>>>>>> 4");

        println!("GENERATED: {}", generated_token.access_token);

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&["test_audience"]);
        let result = jwt.decode::<JwtTokenClaims>(&generated_token.access_token, &validation);

        assert!(result.is_ok());

        let decoded_claims = result.unwrap().claims;

        assert_eq!(decoded_claims.sub, claims.sub);
        assert_eq!(decoded_claims.iat, claims.iat);
        assert_eq!(decoded_claims.exp, claims.exp);
        assert_eq!(decoded_claims.iss, claims.iss);
    }

    #[test]
    fn test_jwt_decode_invalid_token() {
        let (public_key, private_key) = Jwt::dummy_keys();
        let jwt = Jwt::new(public_key, private_key, 60);

        let invalid_token = "invalid_token";

        let result =
            jwt.decode::<JwtTokenClaims>(invalid_token, &Validation::new(Algorithm::RS256));

        assert!(result.is_err());
        // assert_eq!(result.unwrap_err(), &ErrorKind::InvalidToken);
        // if result.unwrap_err() == AppMessage::JwtError {  }

        match result.unwrap_err().downcast::<Error>() {
            Ok(err) => {
                assert_eq!(err, Error::from(ErrorKind::InvalidToken));
            }
            Err(err) => {
                panic!("Expected jsonwebtoken::Error, got something else: {err:?}")
            }
        }
    }
}
