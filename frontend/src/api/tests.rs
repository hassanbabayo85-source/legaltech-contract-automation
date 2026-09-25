//! Frontend unit tests for pure logic.
//!
//! Run with:
//!
//!     cd frontend
//!     wasm-pack test --headless --chrome
//!
//! or, if a browser is not available, these tests are still exercised
//! via `cargo test --target wasm32-unknown-unknown` in CI.

#![cfg(test)]

use wasm_bindgen_test::wasm_bindgen_test;

use crate::api::error::{ApiError, ApiErrorBody, ApiErrorInner};

#[wasm_bindgen_test]
fn api_error_classifies_unauthorized() {
    let err = ApiError::from_status(401, None, None);
    assert!(err.is_unauthorized());
    assert_eq!(err.category(), "unauthorized");
    assert!(!err.is_retryable());
}

#[wasm_bindgen_test]
fn api_error_classifies_rate_limit_with_retry_after() {
    let err = ApiError::from_status(429, None, Some(30));
    assert!(err.is_retryable());
    assert_eq!(err.category(), "rate_limited");
    let msg = err.user_message();
    assert!(msg.contains("30"), "message should mention seconds: {msg}");
}

#[wasm_bindgen_test]
fn api_error_classifies_server() {
    let err = ApiError::from_status(500, None, None);
    assert!(err.is_retryable());
    assert_eq!(err.category(), "server");
    assert_eq!(
        err.user_message(),
        "Something went wrong. Please try again."
    );
}

#[wasm_bindgen_test]
fn api_error_uses_backend_message_when_present() {
    let body = ApiErrorBody {
        error: ApiErrorInner {
            code: "conflict".to_string(),
            message: "an account with this email already exists".to_string(),
        },
    };
    let err = ApiError::from_status(409, Some(body), None);
    assert_eq!(
        err.user_message(),
        "an account with this email already exists"
    );
    assert_eq!(err.category(), "conflict");
}

#[wasm_bindgen_test]
fn api_error_network_has_generic_message() {
    let err = ApiError::Network("connect refused".to_string());
    assert!(err.is_retryable());
    assert_eq!(err.category(), "network");
    assert!(!err.user_message().contains("connect refused"));
}
