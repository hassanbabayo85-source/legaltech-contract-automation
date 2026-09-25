//! Shared application state.
//!
//! * `toasts` — transient notifications and the `ToastHost` renderer.

pub mod toasts;

pub use toasts::{provide_toasts, use_toasts, ToastHost};

pub mod theme;
