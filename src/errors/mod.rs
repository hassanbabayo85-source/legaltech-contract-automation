//! Central application error type.

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::utils::redact::redact_secrets;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("validation error: {0}")]
    Validation(String),

    #[error("authentication error: {0}")]
    Authentication(String),

    #[error("authorization error: {0}")]
    Authorization(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("rate limited: retry after {retry_after_seconds}s")]
    RateLimited { retry_after_seconds: u64 },

    #[error("payload too large")]
    PayloadTooLarge,

    #[error("unprocessable entity: {0}")]
    UnprocessableEntity(String),

    #[error("analysis already in progress")]
    AnalysisInProgress,

    #[error("database error")]
    Database(#[from] sqlx::Error),

    #[error("external service error: {0}")]
    ExternalService(String),

    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::Authentication(_) => StatusCode::UNAUTHORIZED,
            AppError::Authorization(_) => StatusCode::FORBIDDEN,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            AppError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            AppError::UnprocessableEntity(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::AnalysisInProgress => StatusCode::CONFLICT,
            AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ExternalService(_) => StatusCode::BAD_GATEWAY,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_code(&self) -> &'static str {
        match self {
            AppError::Validation(_) => "validation_error",
            AppError::Authentication(_) => "authentication_error",
            AppError::Authorization(_) => "authorization_error",
            AppError::NotFound(_) => "not_found",
            AppError::Conflict(_) => "conflict",
            AppError::RateLimited { .. } => "rate_limited",
            AppError::PayloadTooLarge => "payload_too_large",
            AppError::UnprocessableEntity(_) => "unprocessable_entity",
            AppError::AnalysisInProgress => "analysis_in_progress",
            AppError::Database(_) => "database_error",
            AppError::ExternalService(_) => "external_service_error",
            AppError::Internal(_) => "internal_error",
        }
    }

    fn public_message(&self) -> String {
        match self {
            AppError::Validation(message) => message.clone(),
            AppError::Authentication(message) => message.clone(),
            AppError::Authorization(message) => message.clone(),
            AppError::NotFound(message) => message.clone(),
            AppError::Conflict(message) => message.clone(),
            AppError::RateLimited {
                retry_after_seconds,
            } => {
                format!("too many requests; retry after {retry_after_seconds} seconds")
            }
            AppError::PayloadTooLarge => "request body is too large".to_string(),
            AppError::UnprocessableEntity(message) => message.clone(),
            AppError::AnalysisInProgress => {
                "an analysis for this contract is already in progress".to_string()
            }
            AppError::ExternalService(message) => message.clone(),
            AppError::Database(_) => "an internal error occurred".to_string(),
            AppError::Internal(_) => "an internal error occurred".to_string(),
        }
    }

    pub fn log_message(&self) -> String {
        match self {
            AppError::Validation(message) => {
                format!("validation error: {}", redact_secrets(message))
            }
            AppError::Authentication(message) => {
                format!("authentication error: {}", redact_secrets(message))
            }
            AppError::Authorization(message) => {
                format!("authorization error: {}", redact_secrets(message))
            }
            AppError::NotFound(message) => format!("not found: {}", redact_secrets(message)),
            AppError::Conflict(message) => format!("conflict: {}", redact_secrets(message)),
            AppError::RateLimited {
                retry_after_seconds,
            } => {
                format!("rate limited: retry after {retry_after_seconds}s")
            }
            AppError::PayloadTooLarge => "payload too large".to_string(),
            AppError::UnprocessableEntity(message) => {
                format!("unprocessable entity: {}", redact_secrets(message))
            }
            AppError::AnalysisInProgress => "analysis already in progress".to_string(),
            AppError::ExternalService(message) => {
                format!("external service error: {}", redact_secrets(message))
            }
            AppError::Database(_) => "database error".to_string(),
            AppError::Internal(_) => "internal error".to_string(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let log_message = self.log_message();
        let retry_after = match &self {
            AppError::RateLimited {
                retry_after_seconds,
            } => Some(*retry_after_seconds),
            _ => None,
        };

        if status.is_server_error() {
            tracing::error!(error = %log_message, "request failed with a server error");
        } else {
            tracing::warn!(error = %log_message, "request failed with a client error");
        }

        let body = Json(json!({
            "error": {
                "code": self.error_code(),
                "message": self.public_message(),
            }
        }));

        let mut response = (status, body).into_response();
        if let Some(seconds) = retry_after {
            if let Ok(value) = HeaderValue::from_str(&seconds.to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
        }
        response
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[tokio::test]
    async fn validation_error_maps_to_bad_request() {
        let error = AppError::Validation("field `name` is required".to_string());
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "validation_error");
        assert_eq!(json["error"]["message"], "field `name` is required");
    }

    #[tokio::test]
    async fn not_found_maps_to_404() {
        assert_eq!(
            AppError::NotFound("contract".to_string())
                .into_response()
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn conflict_maps_to_409() {
        assert_eq!(
            AppError::Conflict("already exists".to_string())
                .into_response()
                .status(),
            StatusCode::CONFLICT
        );
    }

    #[tokio::test]
    async fn analysis_in_progress_maps_to_409() {
        let response = AppError::AnalysisInProgress.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "analysis_in_progress");
    }

    #[tokio::test]
    async fn rate_limited_maps_to_429_with_retry_after() {
        let response = AppError::RateLimited {
            retry_after_seconds: 42,
        }
        .into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok()),
            Some("42")
        );
    }

    #[tokio::test]
    async fn payload_too_large_maps_to_413() {
        assert_eq!(
            AppError::PayloadTooLarge.into_response().status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    #[tokio::test]
    async fn unprocessable_entity_maps_to_422() {
        assert_eq!(
            AppError::UnprocessableEntity("unknown field".to_string())
                .into_response()
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[tokio::test]
    async fn internal_error_hides_details_from_client() {
        let error = AppError::Internal(anyhow::anyhow!("leaked db password: hunter2"));
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let message = json["error"]["message"].as_str().unwrap();
        assert!(!message.contains("hunter2"));
        assert_eq!(message, "an internal error occurred");
    }

    #[tokio::test]
    async fn authentication_error_maps_to_401() {
        assert_eq!(
            AppError::Authentication("missing token".to_string())
                .into_response()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn authorization_error_maps_to_403() {
        assert_eq!(
            AppError::Authorization("insufficient role".to_string())
                .into_response()
                .status(),
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn internal_log_message_omits_source_details() {
        let error = AppError::Internal(anyhow::anyhow!("password=hunter2 in request body"));
        assert_eq!(error.log_message(), "internal error");
    }

    #[test]
    fn database_log_message_omits_source_details() {
        let error = AppError::Database(sqlx::Error::RowNotFound);
        assert_eq!(error.log_message(), "database error");
    }

    #[test]
    fn external_service_log_message_redacts_authorization_header() {
        let error = AppError::ExternalService(
            "upstream rejected call: Authorization: Bearer super-secret-token".to_string(),
        );
        let log_message = error.log_message();
        assert!(!log_message.contains("super-secret-token"));
        assert!(log_message.contains("<redacted>"));
    }

    #[test]
    fn validation_log_message_redacts_inline_secret() {
        let error = AppError::Validation("bad field: api_key=abcd1234".to_string());
        assert!(!error.log_message().contains("abcd1234"));
    }
}
