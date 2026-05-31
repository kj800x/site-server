use hmac::{Hmac, Mac};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use time::OffsetDateTime;

pub const TOKEN_TTL_SECONDS: i64 = 60 * 60 * 24 * 30; // 30 days

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub iat: i64,
    pub exp: i64,
}

#[derive(Clone)]
pub struct JwtKeys {
    pub encoding: EncodingKey,
    pub decoding: DecodingKey,
}

impl JwtKeys {
    /// Derive HS256 keys from `JWT_SECRET` if set, else HMAC-SHA256(password, "site-server-jwt-v1").
    /// Returns None when neither source is available.
    pub fn from_password(password: &str) -> Option<Self> {
        let secret_bytes: Vec<u8> = if let Ok(s) = std::env::var("JWT_SECRET") {
            s.into_bytes()
        } else {
            let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(password.as_bytes()).ok()?;
            mac.update(b"site-server-jwt-v1");
            mac.finalize().into_bytes().to_vec()
        };
        Some(JwtKeys {
            encoding: EncodingKey::from_secret(&secret_bytes),
            decoding: DecodingKey::from_secret(&secret_bytes),
        })
    }
}

pub fn issue_token(keys: &JwtKeys, username: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let claims = Claims {
        sub: username.to_string(),
        iat: now,
        exp: now + TOKEN_TTL_SECONDS,
    };
    encode(&Header::new(Algorithm::HS256), &claims, &keys.encoding)
}

pub fn validate_token(keys: &JwtKeys, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let validation = Validation::new(Algorithm::HS256);
    decode::<Claims>(token, &keys.decoding, &validation).map(|d| d.claims)
}
