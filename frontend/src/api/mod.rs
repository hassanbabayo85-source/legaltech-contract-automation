//! HTTP client for the LexHack backend.
//!
//! * [`config`] — build-time `API_BASE_URL`.
//! * [`error`]  — unified `ApiError` and helpers.
//! * [`models`] — typed wire DTOs matching the backend's JSON.
//! * [`client`] — the `ApiClient` itself.

pub mod client;
pub mod config;
pub mod error;
pub mod models;

pub use client::ApiClient;
pub use error::ApiError;

#[cfg(test)]
mod tests;
