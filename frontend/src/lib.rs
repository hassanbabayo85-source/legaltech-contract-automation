//! LexHack frontend (Leptos CSR).
//!
//! Split into modules by responsibility:
//!
//! * `api`         — typed HTTP client for the backend REST API.
//! * `auth`        — session storage, auth context, protected routes.
//! * `components`  — reusable presentational components.
//! * `pages`       — route-level components.
//! * `state`       — shared application state (notifications, theme).
//! * `app`         — the root component and the router.
//!
//! The WASM entry point (`main.rs`) calls [`bootstrap`].

#![allow(clippy::needless_return)]

pub mod api;
pub mod app;
pub mod auth;
pub mod components;
pub mod pages;
pub mod state;

/// Installs the panic hook, tracing, and mounts the root `<App/>`
/// component.
pub fn bootstrap() {
    console_error_panic_hook::set_once();

    tracing_wasm::set_as_global_default();

    leptos::mount::mount_to_body(app::App);
}
