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

// -------------------------------------------------------------------------
// Wire model deserialization — these catch drift between the backend's
// JSON shape and the frontend's typed DTOs before it reaches a browser.
// -------------------------------------------------------------------------

#[wasm_bindgen_test]
fn contract_summary_deserializes_from_backend_json() {
    let json = r#"{
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "title": "Test Agreement",
        "start_date": "2026-01-01",
        "end_date": null,
        "risk_level": "medium",
        "risk_score": 45,
        "risk_summary": "Some risk",
        "content_version": 1,
        "analysis_status": "completed",
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z"
    }"#;
    let parsed: crate::api::models::ContractSummary =
        serde_json::from_str(json).expect("should deserialize");
    assert_eq!(parsed.title, "Test Agreement");
    assert_eq!(parsed.risk_score, Some(45));
    assert_eq!(parsed.end_date, None);
}

#[wasm_bindgen_test]
fn error_envelope_deserializes_from_backend_json() {
    let json = r#"{"error":{"code":"validation_error","message":"title is required"}}"#;
    let parsed: crate::api::models::ErrorEnvelope =
        serde_json::from_str(json).expect("should deserialize");
    assert_eq!(parsed.error.code, "validation_error");
    assert_eq!(parsed.error.message, "title is required");
}

#[wasm_bindgen_test]
fn risk_item_deserializes_from_backend_json() {
    let json = r#"{
        "id": "550e8400-e29b-41d4-a716-446655440001",
        "title": "Unlimited indemnification",
        "description": "Client indemnifies Provider without limit",
        "risk_level": "critical",
        "risk_score": 90,
        "evidence": "Client shall indemnify, defend, and hold harmless Provider..."
    }"#;
    let parsed: crate::api::models::RiskItem =
        serde_json::from_str(json).expect("should deserialize");
    assert_eq!(parsed.risk_level, "critical");
    assert_eq!(parsed.risk_score, 90);
}
