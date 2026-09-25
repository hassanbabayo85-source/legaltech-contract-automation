//! Middleware and cross-cutting HTTP concerns.
//!
//! * Request tracing is installed in `main.rs`.
//! * Security response headers (`X-Content-Type-Options`,
//!   `Referrer-Policy`, `Cache-Control`) are added in `main.rs`.
//! * [`rate_limit`] provides the sliding-window limiter used by auth
//!   and by the AI / channel endpoints.

pub mod rate_limit;
