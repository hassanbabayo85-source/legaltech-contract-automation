//! The channel abstraction and normalized notification payload.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use crate::reminders::types::ChannelType;

/// A normalized notification. Channels translate this into their own
/// wire format. Deliberately does **not** carry `raw_text` or any other
/// bulk legal content: the recipient only needs enough to identify the
/// reminder and act on it.
#[derive(Debug, Clone)]
pub struct Notification {
    pub reminder_id: Uuid,
    pub contract_id: Uuid,
    pub obligation_id: Option<Uuid>,
    pub title: String,
    pub message: String,
    pub due_date: Option<NaiveDate>,
    pub reminder_type: String,
    pub channel_type: ChannelType,
}

impl Notification {
    /// Renders the notification into a plain-text form suitable for
    /// chat providers (Telegram, Discord).
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str("Reminder: ");
        out.push_str(&self.title);
        out.push('\n');
        out.push_str(&self.message);
        if let Some(date) = self.due_date {
            out.push_str(&format!("\nDue: {date}"));
        }
        out.push_str(&format!("\nContract: {}", self.contract_id));
        if let Some(oid) = self.obligation_id {
            out.push_str(&format!("\nObligation: {oid}"));
        }
        out
    }
}

/// Failure modes for a channel delivery attempt.
#[derive(Debug, Clone, thiserror::Error)]
pub enum DeliveryError {
    /// The channel exists but has no credentials / destination
    /// configured. Permanent.
    #[error("channel is not configured")]
    NotConfigured,

    /// The destination URL was rejected by SSRF protection. Permanent.
    #[error("destination rejected by SSRF protection")]
    UnsafeDestination,

    /// The destination URL is malformed. Permanent.
    #[error("invalid destination URL")]
    InvalidUrl,

    /// The request timed out. Retryable.
    #[error("request timed out")]
    Timeout,

    /// Provider returned HTTP 429. Retryable, optionally after the
    /// duration given by `Retry-After`.
    #[error("rate limited")]
    RateLimited { retry_after_seconds: Option<u64> },

    /// Transient network or provider-availability error. Retryable.
    #[error("transient delivery error")]
    Transient,

    /// A 4xx (other than 429) that will not be fixed by retrying.
    /// Permanent.
    #[error("permanent delivery error (status {status})")]
    Permanent { status: u16 },

    /// The channel type in the reminder row is unknown. Permanent.
    #[error("unknown channel type")]
    UnknownChannel,

    /// Anything else. Permanent by default.
    #[error("other delivery error")]
    Other,
}

impl DeliveryError {
    /// True if a bounded retry may succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            DeliveryError::Timeout | DeliveryError::RateLimited { .. } | DeliveryError::Transient
        )
    }

    /// A short, safe category string suitable for
    /// `reminders.last_error` and log fields. Contains no provider
    /// response data and no credentials.
    pub fn safe_category(&self) -> &'static str {
        match self {
            DeliveryError::NotConfigured => "channel_not_configured",
            DeliveryError::UnsafeDestination => "ssrf_blocked",
            DeliveryError::InvalidUrl => "invalid_url",
            DeliveryError::Timeout => "timeout",
            DeliveryError::RateLimited { .. } => "rate_limited",
            DeliveryError::Transient => "transient",
            DeliveryError::Permanent { .. } => "permanent_http_error",
            DeliveryError::UnknownChannel => "unknown_channel",
            DeliveryError::Other => "other",
        }
    }

    /// Retry-After hint, if the provider supplied one.
    pub fn retry_after_seconds(&self) -> Option<u64> {
        match self {
            DeliveryError::RateLimited {
                retry_after_seconds,
            } => *retry_after_seconds,
            _ => None,
        }
    }
}

/// The delivery abstraction the worker depends on.
#[async_trait]
pub trait NotificationChannel: Send + Sync + std::fmt::Debug {
    /// Sends one notification. Returns `Ok(())` only after the provider
    /// has confirmed acceptance.
    async fn send(&self, notification: &Notification) -> Result<(), DeliveryError>;

    /// The channel's human-readable name for logs and audit metadata.
    fn name(&self) -> &'static str;
}

// Silences unused-import warnings in builds where these are only used
// transitively.
#[allow(dead_code)]
fn _documented_types() {
    let _ = Utc::now();
    let _: Option<DateTime<Utc>> = None;
}
