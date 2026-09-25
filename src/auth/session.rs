//! Session token generation, hashing, and lifecycle helpers.

use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Number of random bytes in a session token. 32 bytes = 256 bits of
/// entropy, the standard recommendation for bearer tokens.
const TOKEN_BYTES: usize = 32;

/// Generates a new session token: 32 random bytes, base64url-encoded
/// (no padding). The result is 43 ASCII characters, safe for HTTP
/// headers and URLs.
pub fn generate_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Computes the SHA-256 hash of a raw token, hex-encoded. This is what
/// is stored in `sessions.token_hash` and what lookups use.
pub fn hash_token(raw_token: &str) -> String {
    let digest = Sha256::digest(raw_token.as_bytes());
    hex::encode(digest)
}

/// A freshly issued session: the raw token (returned to the client once)
/// and the values needed to persist the session.
#[derive(Debug, Clone)]
pub struct IssuedToken {
    pub raw_token: String,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
}

/// Generates a fresh token and computes its hash and expiry.
pub fn issue_token(lifetime_seconds: i64) -> IssuedToken {
    let raw_token = generate_token();
    let token_hash = hash_token(&raw_token);
    let expires_at = Utc::now() + Duration::seconds(lifetime_seconds);
    IssuedToken {
        raw_token,
        token_hash,
        expires_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_unique_and_url_safe() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
        assert!(!a.contains('+') && !a.contains('/') && !a.contains('='));
        assert_eq!(a.len(), 43);
    }

    #[test]
    fn hash_is_deterministic_and_64_hex_chars() {
        let h1 = hash_token("abc123");
        let h2 = hash_token("abc123");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
        assert!(h1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn distinct_tokens_have_distinct_hashes() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(hash_token(&a), hash_token(&b));
    }

    #[test]
    fn issued_token_round_trips() {
        let issued = issue_token(3600);
        assert_eq!(hash_token(&issued.raw_token), issued.token_hash);
        assert!(issued.expires_at > Utc::now());
    }
}
