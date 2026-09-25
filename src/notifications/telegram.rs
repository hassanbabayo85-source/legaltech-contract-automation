//! Telegram Bot API channel.
//!
//! Sends `sendMessage` requests to
//! `https://api.telegram.org/bot<token>/sendMessage`. The bot token is
//! read from configuration and is **never** logged or included in
//! error messages.

use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::notifications::channel::{DeliveryError, Notification, NotificationChannel};

/// A Telegram delivery channel, configured from environment.
///
/// Manual `Debug`: the bot token is a credential and must never
/// appear in logs.
pub struct TelegramChannel {
    client: Client,
    bot_token: Option<String>,
    chat_id: Option<String>,
}

impl std::fmt::Debug for TelegramChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramChannel")
            .field("client", &"<reqwest::Client>")
            .field("bot_token", &self.bot_token.as_ref().map(|_| "<redacted>"))
            .field("chat_id", &self.chat_id.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl TelegramChannel {
    /// Builds a Telegram channel. If either `bot_token` or `chat_id` is
    /// missing, [`send`] returns [`DeliveryError::NotConfigured`]
    /// without making a network call.
    pub fn new(client: Client, bot_token: Option<String>, chat_id: Option<String>) -> Self {
        Self {
            client,
            bot_token,
            chat_id,
        }
    }

    fn endpoint(&self, token: &str) -> String {
        format!("https://api.telegram.org/bot{token}/sendMessage")
    }
}

#[async_trait]
impl NotificationChannel for TelegramChannel {
    fn name(&self) -> &'static str {
        "telegram"
    }

    async fn send(&self, notification: &Notification) -> Result<(), DeliveryError> {
        let token = self
            .bot_token
            .as_deref()
            .ok_or(DeliveryError::NotConfigured)?;
        let chat_id = self
            .chat_id
            .as_deref()
            .ok_or(DeliveryError::NotConfigured)?;

        let body = json!({
            "chat_id": chat_id,
            "text": notification.render_text(),
            "disable_web_page_preview": true,
        });

        let response = self
            .client
            .post(self.endpoint(token))
            .header("Idempotency-Key", notification.reminder_id.to_string())
            .json(&body)
            .send()
            .await
            .map_err(classify_reqwest_error)?;

        classify_response(response, "telegram").await
    }
}

/// Shared classification for reqwest-level failures (no response yet).
pub(crate) fn classify_reqwest_error(err: reqwest::Error) -> DeliveryError {
    if err.is_timeout() {
        DeliveryError::Timeout
    } else if err.is_connect() || err.is_request() {
        DeliveryError::Transient
    } else {
        DeliveryError::Other
    }
}

/// Shared response classification for JSON APIs (Telegram, Discord,
/// generic webhook).
pub(crate) async fn classify_response(
    response: reqwest::Response,
    _channel_name: &'static str,
) -> Result<(), DeliveryError> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }

    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok());
        return Err(DeliveryError::RateLimited {
            retry_after_seconds: retry_after,
        });
    }

    if status.is_server_error() {
        return Err(DeliveryError::Transient);
    }

    Err(DeliveryError::Permanent {
        status: status.as_u16(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reminders::types::ChannelType;
    use uuid::Uuid;

    fn n() -> Notification {
        Notification {
            reminder_id: Uuid::new_v4(),
            contract_id: Uuid::new_v4(),
            obligation_id: None,
            title: "Payment due".to_string(),
            message: "Pay the invoice within 30 days.".to_string(),
            due_date: None,
            reminder_type: "on_deadline".to_string(),
            channel_type: ChannelType::Telegram,
        }
    }

    #[test]
    fn render_text_includes_title_and_ids() {
        let n = n();
        let s = n.render_text();
        assert!(s.contains("Payment due"));
        assert!(s.contains(&n.contract_id.to_string()));
    }

    #[test]
    fn endpoint_contains_bot_prefix() {
        let ch = TelegramChannel::new(Client::new(), Some("123:abc".into()), Some("1".into()));
        let url = ch.endpoint("123:abc");
        assert!(url.starts_with("https://api.telegram.org/bot"));
        assert!(url.ends_with("/sendMessage"));
    }

    #[tokio::test]
    async fn not_configured_returns_permanent_error() {
        let ch = TelegramChannel::new(Client::new(), None, None);
        let err = ch.send(&n()).await.unwrap_err();
        assert_eq!(err.safe_category(), "channel_not_configured");
        assert!(!err.is_retryable());
    }
}
