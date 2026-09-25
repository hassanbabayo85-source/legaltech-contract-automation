//! The `AiProvider` trait and its error type.

use async_trait::async_trait;

use crate::ai::schema::AiAnalysis;

/// A pluggable contract-analysis provider.
///
/// Implementations are `Send + Sync` and cheaply shareable behind an
/// `Arc`. The trait is object-safe so `AppState` can hold
/// `Arc<dyn AiProvider>` and swap implementations at startup.
#[async_trait]
pub trait AiProvider: Send + Sync + std::fmt::Debug {
    /// Analyzes `raw_text` and returns the structured result.
    ///
    /// Implementations must NOT log `raw_text`, and must NOT include it
    /// in any error returned to the caller.
    async fn analyze_contract(&self, raw_text: &str) -> Result<AiAnalysis, AiError>;

    /// The provider's name for logging and analysis metadata. Must be a
    /// short ASCII identifier (e.g. `"openai"`).
    fn name(&self) -> &'static str;

    /// The model identifier for logging and analysis metadata.
    fn model(&self) -> &str;
}

/// Failure modes when talking to an AI provider.
///
/// The variants are deliberately coarse: the caller needs to decide
/// (a) whether to retry, and (b) what safe string to record in
/// `contracts.analysis_error`. Nothing here is ever surfaced to the
/// client verbatim.
#[derive(Debug, Clone, thiserror::Error)]
pub enum AiError {
    /// No API key / base URL configured. Retrying will not help.
    #[error("ai provider is not configured")]
    NotConfigured,

    /// The request exceeded the configured timeout. Retryable.
    #[error("ai request timed out")]
    Timeout,

    /// The provider returned HTTP 429. Retryable.
    #[error("ai provider rate limited")]
    RateLimited,

    /// Network or provider-availability error. Retryable.
    #[error("transient ai provider error")]
    Transient,

    /// HTTP 4xx (other than 429) or an unambiguously permanent error.
    /// Not retryable. Carries the HTTP status so callers can record
    /// and display exactly which 4xx happened.
    #[error("permanent ai provider error (HTTP {status})")]
    Permanent { status: u16 },

    /// The provider's response did not parse as our expected JSON shape.
    /// Not retryable (the model will produce the same output).
    #[error("malformed ai response")]
    MalformedResponse,

    /// Anything else. Not retryable by default.
    #[error("ai provider error")]
    Other,
}

impl AiError {
    /// True if a bounded retry may succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            AiError::Timeout | AiError::RateLimited | AiError::Transient
        )
    }

    /// A short, safe string suitable for `contracts.analysis_error` and
    /// log messages. Contains no provider response data, no contract
    /// text, no credentials.
    pub fn safe_category(&self) -> String {
        match self {
            AiError::NotConfigured => "not_configured".to_string(),
            AiError::Timeout => "timeout".to_string(),
            AiError::RateLimited => "rate_limited".to_string(),
            AiError::Transient => "transient".to_string(),
            AiError::Permanent { status } => format!("permanent_http_{status}"),
            AiError::MalformedResponse => "malformed_response".to_string(),
            AiError::Other => "other".to_string(),
        }
    }
}
