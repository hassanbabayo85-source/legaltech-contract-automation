//! WASM entry point for the LexHack frontend.
//!
//! Bootstraps the Leptos CSR application. Everything else lives in
//! `lib.rs` and its modules so that `wasm-bindgen-test` can import
//! component code without going through `main`.

fn main() {
    lexhack_frontend::bootstrap();
}
