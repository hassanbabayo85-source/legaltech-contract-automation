//! Argon2id password hashing and verification.
//!
//! The Argon2 parameters are read from [`Config`] (which validates them
//! at load time). Centralizing the construction here means there is
//! exactly one place to change cost parameters, and the same instance is
//! used for both hashing and verification — otherwise verification would
//! silently fail whenever the parameters changed.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::rngs::OsRng;

use crate::config::Config;

/// Builds the Argon2id instance used for hashing and verification from
/// the application configuration.
pub fn build_argon2(config: &Config) -> Argon2<'static> {
    let params = Params::new(
        config.argon2_m_cost,
        config.argon2_t_cost,
        config.argon2_p_cost,
        None,
    )
    .expect("Argon2 parameters were validated at config load time");

    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Hashes `password` with Argon2id and returns the PHC-format string
/// suitable for storing in `users.password_hash`.
///
/// The output includes the algorithm, version, cost parameters, and salt,
/// so future parameter changes remain compatible with existing hashes.
pub fn hash_password(
    config: &Config,
    password: &str,
) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(OsRng);
    let argon2 = build_argon2(config);
    let hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// Verifies `password` against a stored PHC-format hash.
///
/// Returns `true` if the password matches. The comparison is
/// constant-time (implemented inside the `argon2` crate). Returns `false`
/// — rather than an error — if the stored hash is malformed, because
/// that is a data-integrity problem for logging, not a 500 for the
/// caller.
pub fn verify_password(config: &Config, password: &str, stored_hash: &str) -> bool {
    let parsed = match PasswordHash::new(stored_hash) {
        Ok(parsed) => parsed,
        Err(_) => return false,
    };
    let argon2 = build_argon2(config);
    argon2.verify_password(password.as_bytes(), &parsed).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_config() -> Config {
        let pairs: HashMap<&str, &str> = [
            ("DATABASE_URL", "postgres://x:x@localhost/x"),
            ("ARGON2_M_COST", "19456"),
            ("ARGON2_T_COST", "2"),
            ("ARGON2_P_COST", "1"),
        ]
        .into_iter()
        .collect();
        Config::from_lookup(|k| pairs.get(k).map(|v| v.to_string())).unwrap()
    }

    #[test]
    fn hash_is_phc_argon2id() {
        let config = test_config();
        let hash = hash_password(&config, "correct horse battery staple").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert_ne!(hash, "correct horse battery staple");
    }

    #[test]
    fn same_password_hashes_differently_each_time() {
        let config = test_config();
        let a = hash_password(&config, "correct horse battery staple").unwrap();
        let b = hash_password(&config, "correct horse battery staple").unwrap();
        assert_ne!(a, b, "salts must differ between hashes");
    }

    #[test]
    fn verify_accepts_correct_password() {
        let config = test_config();
        let hash = hash_password(&config, "correct horse battery staple").unwrap();
        assert!(verify_password(
            &config,
            "correct horse battery staple",
            &hash
        ));
    }

    #[test]
    fn verify_rejects_wrong_password() {
        let config = test_config();
        let hash = hash_password(&config, "correct horse battery staple").unwrap();
        assert!(!verify_password(&config, "not the password", &hash));
    }

    #[test]
    fn verify_rejects_malformed_hash() {
        let config = test_config();
        assert!(!verify_password(&config, "anything", "not-a-phc-string"));
    }
}
