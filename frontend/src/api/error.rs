//! Unified error type for API calls.
//!
//! The backend's `AppError` returns a JSON body of the shape
//! `{"error":{"code":"...","message":"..."}}`. This module turns that
//! into a typed error, and provides a small amount of classification so
//! the UI can react sensibly (e.g. redirect on 401, back off on 429).

use serde::Deserialize;

/// A `{"error": {"code": "...", "message": "..."}}` response body.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorBody {
    pub error: ApiErrorInner,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorInner {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// Network failure, DNS failure, CORS preflight failure, or the
    /// browser refusing the request. The inner string is a
    /// human-readable summary. There is no HTTP status.
    Network(String),

    /// HTTP 400 — the client sent something invalid.
    Validation { code: String, message: String },

    /// HTTP 401 — no session, or the session expired/was revoked.
    Unauthorized { code: String, message: String },

    /// HTTP 403 — authenticated but not permitted.
    Forbidden { code: String, message: String },

    /// HTTP 404 — the resource does not exist or is not visible to the
    /// caller.
    NotFound { code: String, message: String },

    /// HTTP 409 — the request conflicts with current server state
    /// (duplicate channel, analysis in progress, etc.).
    Conflict { code: String, message: String },

    /// HTTP 413 — the request body exceeded the server's limit.
    PayloadTooLarge { code: String, message: String },

    /// HTTP 422 — the JSON body was structurally invalid (unknown
    /// field, wrong type).
    UnprocessableEntity { code: String, message: String },

    /// HTTP 429 — rate limited. `retry_after_seconds` comes from the
    /// `Retry-After` header if the server sent one.
    RateLimited {
        code: String,
        message: String,
        retry_after_seconds: Option<u64>,
    },

    /// HTTP 5xx — server-side failure.
    Server {
        status: u16,
        code: String,
        message: String,
    },

    /// Any other unexpected response.
    Other { status: u16, message: String },
}

impl ApiError {
    /// Builds an `ApiError` from an HTTP status, the parsed body (if
    /// any), and an optional `Retry-After` header.
    pub fn from_status(
        status: u16,
        body: Option<ApiErrorBody>,
        retry_after_seconds: Option<u64>,
    ) -> Self {
        let code = body
            .as_ref()
            .map(|b| b.error.code.clone())
            .unwrap_or_default();
        let message = body
            .as_ref()
            .map(|b| b.error.message.clone())
            .unwrap_or_else(|| fallback_message(status));

        match status {
            400 => Self::Validation { code, message },
            401 => Self::Unauthorized { code, message },
            403 => Self::Forbidden { code, message },
            404 => Self::NotFound { code, message },
            409 => Self::Conflict { code, message },
            413 => Self::PayloadTooLarge { code, message },
            422 => Self::UnprocessableEntity { code, message },
            429 => Self::RateLimited {
                code,
                message,
                retry_after_seconds,
            },
            500..=599 => Self::Server {
                status,
                code,
                message,
            },
            _ => Self::Other { status, message },
        }
    }

    /// True if the caller should treat this as "user is no longer
    /// authenticated" and redirect to login.
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Self::Unauthorized { .. })
    }

    /// True if the request may succeed on retry without changing
    /// anything.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Network(_) | Self::RateLimited { .. } | Self::Server { .. }
        )
    }

    /// A user-facing message. Never includes provider internals, SQL
    /// details, or secrets.
    pub fn user_message(&self) -> String {
        match self {
            Self::Network(_) => {
                "Could not reach the server. Check your connection and try again.".to_string()
            }
            Self::Validation { message, .. }
            | Self::Forbidden { message, .. }
            | Self::NotFound { message, .. }
            | Self::Conflict { message, .. }
            | Self::PayloadTooLarge { message, .. }
            | Self::UnprocessableEntity { message, .. } => message.clone(),
            Self::Unauthorized { .. } => {
                "Your session has expired. Please sign in again.".to_string()
            }
            Self::RateLimited {
                retry_after_seconds: Some(s),
                ..
            } => format!("Too many requests. Please wait {s} seconds and try again."),
            Self::RateLimited { .. } => {
                "Too many requests. Please wait a moment and try again.".to_string()
            }
            Self::Server { .. } | Self::Other { .. } => {
                "Something went wrong. Please try again.".to_string()
            }
        }
    }

    /// A short machine-readable identifier for logs and debugging.
    /// Not shown to the end user.
    pub fn category(&self) -> &'static str {
        match self {
            Self::Network(_) => "network",
            Self::Validation { .. } => "validation",
            Self::Unauthorized { .. } => "unauthorized",
            Self::Forbidden { .. } => "forbidden",
            Self::NotFound { .. } => "not_found",
            Self::Conflict { .. } => "conflict",
            Self::PayloadTooLarge { .. } => "payload_too_large",
            Self::UnprocessableEntity { .. } => "unprocessable_entity",
            Self::RateLimited { .. } => "rate_limited",
            Self::Server { .. } => "server",
            Self::Other { .. } => "other",
        }
    }
}

fn fallback_message(status: u16) -> String {
    match status {
        400 => "The request was invalid.".to_string(),
        401 => "Please sign in.".to_string(),
        403 => "You do not have access to that resource.".to_string(),
        404 => "Not found.".to_string(),
        409 => "There was a conflict with an existing resource.".to_string(),
        413 => "The request was too large.".to_string(),
        422 => "The request could not be processed.".to_string(),
        429 => "Too many requests.".to_string(),
        500..=599 => "The server encountered an error.".to_string(),
        _ => format!("Unexpected response (HTTP {status})."),
    }
}
