//! Persistent storage for the session token.
//!
//! # Security rationale
//!
//! The backend issues an opaque bearer token; the frontend treats it as
//! sensitive. `localStorage` is used only because the SPA must survive
//! a page refresh — a session in `sessionStorage` or in memory would
//! log the user out on reload.
//!
//! `localStorage` is **vulnerable to XSS**. That is why this codebase
//! never renders untrusted content as HTML (`inner_html`, `dangerously_set_inner_html`,
//! etc. — see `components::SafeText`). If an attacker can execute
//! arbitrary JS on the page, no client-side storage is safe.
//!
//! This is documented in `docs/FRONTEND_SECURITY.md`.

use web_sys::window;

const TOKEN_KEY: &str = "lexhack.session.token";

/// Reads the stored session token, if any.
pub fn read_token() -> Option<String> {
    window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(TOKEN_KEY).ok().flatten())
        .filter(|t| !t.is_empty())
}

/// Stores the session token.
pub fn write_token(token: &str) {
    if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(TOKEN_KEY, token);
    }
}

/// Removes the stored session token.
pub fn clear_token() {
    if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.remove_item(TOKEN_KEY);
    }
}
