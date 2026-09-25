//! Best-effort redaction of common secret patterns from strings that are
//! about to be written to logs.
//!
//! This is **defense-in-depth, not a guarantee**. Code that handles secrets
//! (API keys, tokens, passwords, cookies, connection strings) should not
//! pass them into tracing fields at all — see [`crate::errors`] for how the
//! application error type applies this redaction before logging.
//!
//! # Recognized patterns
//!
//! * `Bearer <token>` (case-insensitive) — the token is replaced.
//! * `<key>=<value>`, `<key>: <value>`, `<key>:<value>` — the value is
//!   replaced, where `<key>` is one of a small set of well-known sensitive
//!   names (`password`, `token`, `api_key`, `authorization`, `cookie`, …).
//!   Recognized even when the key is part of a larger token such as
//!   `/v1?api_key=…` or `header.authorization:…`.
//! * A standalone `<key>:` or `<key>=` token followed by whitespace — the
//!   next whitespace-separated token is replaced.
//!
//! # Non-goals
//!
//! * Structured formats (JSON, YAML) are not parsed. A secret inside a
//!   quoted JSON string will only be redacted if it happens to match one of
//!   the inline patterns above.
//! * Whitespace between tokens is normalized to single spaces. Logs are not
//!   a structured transport, so this is acceptable.

/// Replaces recognized secret values in `input` with the literal string
/// `"<redacted>"`.
///
/// See the module documentation for exactly which patterns are recognized.
/// The function never panics and always returns a valid `String`.
pub fn redact_secrets(input: &str) -> String {
    const SENSITIVE_KEYS: &[&str] = &[
        "password",
        "passwd",
        "pwd",
        "secret",
        "client_secret",
        "token",
        "access_token",
        "refresh_token",
        "id_token",
        "api_key",
        "apikey",
        "api-key",
        "authorization",
        "cookie",
        "set-cookie",
        "private_key",
    ];

    let mut out = String::with_capacity(input.len());
    let mut next_is_secret = false;
    let mut first = true;

    for token in input.split_whitespace() {
        if !first {
            out.push(' ');
        }
        first = false;

        let lower = token.to_ascii_lowercase();

        if next_is_secret {
            out.push_str("<redacted>");
            // A redacted `Bearer` implies the actual credential follows.
            next_is_secret = lower == "bearer";
            continue;
        }

        // `Bearer` — the following token is the credential.
        if lower == "bearer" {
            out.push_str(token);
            next_is_secret = true;
            continue;
        }

        // A standalone `<key>:` or `<key>=` token — the following token is
        // the value.
        if (token.ends_with(':') || token.ends_with('=')) && lower.len() >= 2 {
            let key_part = &lower[..lower.len() - 1];
            if SENSITIVE_KEYS.contains(&key_part) {
                out.push_str(token);
                next_is_secret = true;
                continue;
            }
        }

        // An inline `<key>=<value>` or `<key>:<value>` inside a larger
        // token (e.g. a URL query string or a header dump).
        let mut inline: Option<String> = None;
        for sep in ['=', ':'] {
            if let Some(pos) = token.find(sep) {
                let key_part = &lower[..pos];
                let last_segment = key_part
                    .rsplit(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                    .next()
                    .unwrap_or("");
                if SENSITIVE_KEYS.contains(&last_segment) {
                    let mut r = String::with_capacity(token.len());
                    r.push_str(&token[..pos + 1]);
                    r.push_str("<redacted>");
                    inline = Some(r);
                    break;
                }
            }
        }
        if let Some(r) = inline {
            out.push_str(&r);
            continue;
        }

        out.push_str(token);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_password_equals_value() {
        assert_eq!(redact_secrets("password=hunter2"), "password=<redacted>");
    }

    #[test]
    fn redacts_standalone_key_with_colon() {
        assert_eq!(redact_secrets("password: hunter2"), "password: <redacted>");
    }

    #[test]
    fn redacts_bearer_token() {
        assert_eq!(redact_secrets("Bearer abc123"), "Bearer <redacted>");
    }

    #[test]
    fn redacts_authorization_header_with_bearer() {
        let output = redact_secrets("Authorization: Bearer abc123");
        assert!(!output.contains("abc123"), "leaked token: {output}");
        assert!(
            output.contains("<redacted>"),
            "no redaction marker: {output}"
        );
    }

    #[test]
    fn redacts_api_key_in_query_string() {
        let output = redact_secrets("GET /v1?api_key=secretvalue");
        assert!(!output.contains("secretvalue"), "leaked key: {output}");
        assert!(output.contains("<redacted>"));
    }

    #[test]
    fn leaves_non_sensitive_content_alone() {
        let input = "request succeeded in 42ms";
        assert_eq!(redact_secrets(input), input);
    }

    #[test]
    fn handles_empty_input() {
        assert_eq!(redact_secrets(""), "");
    }

    #[test]
    fn redacts_cookie_header() {
        let output = redact_secrets("Cookie: session=abcdef123456");
        assert!(!output.contains("abcdef123456"), "leaked cookie: {output}");
    }
}
