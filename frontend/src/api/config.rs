//! API base URL configuration.
//!
//! The value is baked in at build time so the WASM bundle knows where
//! to send requests. There is no runtime secret here — the API base
//! URL is a public value that is safe to ship to the browser.
//!
//! Override at build time:
//!
//! ```bash
//! API_BASE_URL=https://api.lexhack.example.com trunk build --release
//! ```
//!
//! Defaults to `http://localhost:3000`, which matches the backend's
//! default `PORT=3000` in development.

pub const API_BASE_URL: &str = match option_env!("API_BASE_URL") {
    Some(url) => url,
    None => "http://localhost:3000",
};
