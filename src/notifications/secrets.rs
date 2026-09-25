//! Application-level encryption for channel credentials.
//!
//! Every notification-channel row stores its sensitive fields as JSON,
//! encrypted with ChaCha20-Poly1305 (an AEAD). The 256-bit key comes
//! from `NOTIFICATION_SECRET_KEY`, which must be present at startup in
//! production — the server refuses to boot without it. This is
//! deliberate: a deployment that forgets the key must not silently
//! fall back to plaintext storage.
//!
//! # Format
//!
//! A stored ciphertext is `nonce(12) || ciphertext_and_tag(n)`. A fresh
//! random 96-bit nonce is generated for every encryption operation;
//! nonces are never reused.
//!
//! # Key management
//!
//! Key rotation is not supported in this Part. See `docs/SECURITY.md`
//! for the operational guidance.

use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use rand::RngCore;
use std::sync::Arc;

/// The application's notification secret key.
///
/// Cheaply cloneable. Never formatted via `Display` or `Debug` with
/// the raw bytes — the `Debug` impl below redacts the material.
#[derive(Clone)]
pub struct SecretKey {
    inner: Arc<ChaCha20Poly1305>,
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretKey")
            .field("material", &"<redacted>")
            .finish()
    }
}

/// Errors produced by the encryption layer. Never surfaces key
/// material; only category strings.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("invalid secret key (must be 32 bytes, base64-encoded)")]
    InvalidKey,

    #[error("decryption failed (wrong key or corrupted ciphertext)")]
    DecryptFailed,

    #[error("encryption failed")]
    EncryptFailed,

    #[error("config is not valid JSON")]
    InvalidJson,
}

impl SecretKey {
    /// Parses a base64-encoded 32-byte key.
    pub fn from_base64(encoded: &str) -> Result<Self, SecretError> {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|_| SecretError::InvalidKey)?;
        if raw.len() != 32 {
            return Err(SecretError::InvalidKey);
        }
        let cipher = ChaCha20Poly1305::new_from_slice(&raw).map_err(|_| SecretError::InvalidKey)?;
        Ok(Self {
            inner: Arc::new(cipher),
        })
    }

    /// Encrypts `plaintext`. Returns `nonce || ciphertext+tag`.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, SecretError> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .inner
            .encrypt(nonce, plaintext)
            .map_err(|_| SecretError::EncryptFailed)?;

        let mut out = Vec::with_capacity(12 + ciphertext.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Decrypts data produced by [`SecretKey::encrypt`].
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, SecretError> {
        if data.len() < 12 {
            return Err(SecretError::DecryptFailed);
        }
        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        self.inner
            .decrypt(nonce, ciphertext)
            .map_err(|_| SecretError::DecryptFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_bytes() -> [u8; 32] {
        [7u8; 32]
    }

    fn key() -> SecretKey {
        let encoded = base64::engine::general_purpose::STANDARD.encode(key_bytes());
        SecretKey::from_base64(&encoded).unwrap()
    }

    #[test]
    fn from_base64_rejects_wrong_length() {
        assert!(SecretKey::from_base64("aGVsbG8=").is_err());
    }

    #[test]
    fn from_base64_rejects_invalid_base64() {
        assert!(SecretKey::from_base64("not base64!!!!").is_err());
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let k = key();
        let plain = b"{\"bot_token\":\"secret\"}";
        let ct = k.encrypt(plain).unwrap();
        assert_ne!(&ct[..], plain.as_slice());
        let back = k.decrypt(&ct).unwrap();
        assert_eq!(back, plain);
    }

    #[test]
    fn decrypt_fails_with_wrong_key() {
        let k1 = key();
        let encoded2 = base64::engine::general_purpose::STANDARD.encode([9u8; 32]);
        let k2 = SecretKey::from_base64(&encoded2).unwrap();
        let ct = k1.encrypt(b"top secret").unwrap();
        assert!(k2.decrypt(&ct).is_err());
    }

    #[test]
    fn decrypt_fails_on_truncated_input() {
        let k = key();
        assert!(k.decrypt(&[0u8; 5]).is_err());
    }

    #[test]
    fn nonces_are_unique() {
        let k = key();
        let a = k.encrypt(b"same").unwrap();
        let b = k.encrypt(b"same").unwrap();
        assert_ne!(a[..12], b[..12], "nonces must be unique per encryption");
    }

    #[test]
    fn debug_impl_redacts_material() {
        let k = key();
        assert!(format!("{k:?}").contains("<redacted>"));
    }
}
