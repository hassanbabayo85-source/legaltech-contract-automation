//! `GET /health` — a minimal liveness check.
//!
//! Intentionally does not check database connectivity or any downstream
//! dependency: it only answers "is the process up and serving requests".
//! A separate readiness check can be added in a later part if needed.

use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    #[tokio::test]
    async fn health_returns_ok_status_and_body() {
        let response = health().await;
        let json = response.0;

        assert_eq!(json.status, "ok");
    }

    #[tokio::test]
    async fn health_response_serializes_as_expected() {
        let response = health().await.into_response();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["status"], "ok");
    }
}
