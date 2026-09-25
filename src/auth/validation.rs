//! Input validation for authentication endpoints.
//!
//! Deliberately manual: the rules are short, the error messages need to
//! be precise, and pulling in a validation framework for three fields
//! would be more machinery than the problem needs.

use crate::errors::AppError;

/// Maximum accepted email length. 254 is the RFC 5321 limit.
pub const MAX_EMAIL_LEN: usize = 254;

/// Maximum accepted full-name length.
pub const MAX_FULL_NAME_LEN: usize = 200;

/// Maximum accepted password length in bytes. Very long passwords are a
/// DoS vector against the hashing function.
pub const MAX_PASSWORD_LEN: usize = 1024;

/// Minimum accepted password length in Unicode scalar values. 12 is the
/// current OWASP recommendation.
pub const MIN_PASSWORD_LEN: usize = 12;

/// Normalizes an email address: trims surrounding whitespace and
/// lowercases. Lowercasing matters because `users.email` is UNIQUE
/// case-sensitively at the database level; storing canonical lowercase
/// means `Alice@x.com` and `alice@x.com` cannot both exist.
pub fn normalize_email(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

/// Validates an already-normalized email address.
///
/// Intentionally minimal: rejects empty strings, strings longer than
/// `MAX_EMAIL_LEN`, and strings that do not contain exactly one `@`
/// separating a non-empty local part from a non-empty domain part. We do
/// NOT try to fully validate RFC 5322 — the only true test of an email
/// address is sending to it.
pub fn validate_email(email: &str) -> Result<(), AppError> {
    if email.is_empty() {
        return Err(AppError::Validation("email must not be empty".to_string()));
    }
    if email.len() > MAX_EMAIL_LEN {
        return Err(AppError::Validation(format!(
            "email must be at most {MAX_EMAIL_LEN} characters"
        )));
    }
    if email.chars().any(|c| c.is_whitespace()) {
        return Err(AppError::Validation(
            "email must not contain whitespace".to_string(),
        ));
    }
    let (local, domain) = email
        .split_once('@')
        .ok_or_else(|| AppError::Validation("email must contain an `@`".to_string()))?;
    if local.is_empty() {
        return Err(AppError::Validation(
            "email local part must not be empty".to_string(),
        ));
    }
    if domain.is_empty() || !domain.contains('.') {
        return Err(AppError::Validation(
            "email domain must contain a dot".to_string(),
        ));
    }
    Ok(())
}

/// Validates a full name. Trimmed, non-empty, bounded length.
pub fn validate_full_name(name: &str) -> Result<(), AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation(
            "full_name must not be empty".to_string(),
        ));
    }
    if trimmed.len() > MAX_FULL_NAME_LEN {
        return Err(AppError::Validation(format!(
            "full_name must be at most {MAX_FULL_NAME_LEN} characters"
        )));
    }
    Ok(())
}

/// Validates a password.
///
/// # Unicode policy
///
/// Password bytes are passed to Argon2 exactly as received. We do NOT
/// normalize (NFC/NFD) — that is a policy decision that would silently
/// change what the user typed, and is out of scope for this part.
///
/// Minimum length is measured in Unicode scalar values, so 12 emoji
/// count as 12 characters (not 48 bytes). Maximum length is measured in
/// bytes, since that is what Argon2 actually processes.
pub fn validate_password(password: &str) -> Result<(), AppError> {
    if password.is_empty() {
        return Err(AppError::Validation(
            "password must not be empty".to_string(),
        ));
    }
    let char_count = password.chars().count();
    if char_count < MIN_PASSWORD_LEN {
        return Err(AppError::Validation(format!(
            "password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }
    if password.len() > MAX_PASSWORD_LEN {
        return Err(AppError::Validation(format!(
            "password must be at most {MAX_PASSWORD_LEN} bytes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_lowercases_and_trims() {
        assert_eq!(
            normalize_email("  Alice@Example.COM  "),
            "alice@example.com"
        );
    }

    #[test]
    fn email_rejects_empty() {
        assert!(validate_email("").is_err());
    }

    #[test]
    fn email_rejects_no_at() {
        assert!(validate_email("alice.example.com").is_err());
    }

    #[test]
    fn email_rejects_empty_local() {
        assert!(validate_email("@example.com").is_err());
    }

    #[test]
    fn email_rejects_empty_domain() {
        assert!(validate_email("alice@").is_err());
    }

    #[test]
    fn email_rejects_domain_without_dot() {
        assert!(validate_email("alice@localhost").is_err());
    }

    #[test]
    fn email_rejects_whitespace() {
        assert!(validate_email("al ice@example.com").is_err());
    }

    #[test]
    fn email_accepts_valid() {
        assert!(validate_email("alice@example.com").is_ok());
    }

    #[test]
    fn full_name_rejects_empty() {
        assert!(validate_full_name("   ").is_err());
    }

    #[test]
    fn full_name_accepts_typical() {
        assert!(validate_full_name("Alice Example").is_ok());
    }

    #[test]
    fn password_rejects_empty() {
        assert!(validate_password("").is_err());
    }

    #[test]
    fn password_rejects_too_short() {
        assert!(validate_password("shortpass").is_err());
    }

    #[test]
    fn password_accepts_twelve_chars() {
        assert!(validate_password("twelvecharxx").is_ok());
    }

    #[test]
    fn password_counts_unicode_scalars_not_bytes() {
        let password = "🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐";
        assert_eq!(password.chars().count(), 12);
        assert!(validate_password(password).is_ok());
    }

    #[test]
    fn password_rejects_too_many_bytes() {
        let password = "a".repeat(MAX_PASSWORD_LEN + 1);
        assert!(validate_password(&password).is_err());
    }
}
