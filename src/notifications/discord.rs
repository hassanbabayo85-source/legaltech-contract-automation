//! Discord webhook channel.
//!
//! Posts a JSON body to a pre-configured Discord webhook URL. The URL
//! is a credential (anyone who knows it can post); it is never logged
//! or included in error messages.

use async_trait::async_trait;
use reqwest::{Client, Url};
use serde_json::json;

use crate::notifications::channel::{DeliveryError, Notification, NotificationChannel};
use crate::notifications::telegram::{classify_reqwest_error, classify_response};

/// A Discord delivery channel, configured from environment.
///
/// Manual `Debug`: the webhook URL is a credential (anyone with it
/// can post) and must never appear in logs.
pub struct DiscordChannel {
    client: Client,
    webhook_url: Option<Url>,
}

impl std::fmt::Debug for DiscordChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscordChannel")
            .field("client", &"<reqwest::Client>")
            .field(
                "webhook_url",
                &self.webhook_url.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl DiscordChannel {
    /// Parses `webhook_url` into a [`Url`]. Malformed URLs are stored
    /// as `None` and surface as [`DeliveryError::NotConfigured`] on
    /// send — never as a startup crash, since a misconfigured Discord
    /// URL should not prevent the server from booting.
    pub fn new(client: Client, webhook_url: Option<String>) -> Self {
        let parsed = webhook_url.and_then(|s| Url::parse(&s).ok());
        Self {
            client,
            webhook_url: parsed,
        }
    }
}

#[async_trait]
impl NotificationChannel for DiscordChannel {
    fn name(&self) -> &'static str {
        "discord"
    }

    async fn send(&self, notification: &Notification) -> Result<(), DeliveryError> {
        let url = self
            .webhook_url
            .as_ref()
            .ok_or(DeliveryError::NotConfigured)?;

        let body = json!({
            "content": notification.render_text(),
            "username": "LexHack",
        });

        let response = self
            .client
            .post(url.clone())
            .header("Idempotency-Key", notification.reminder_id.to_string())
            .json(&body)
            .send()
            .await
            .map_err(classify_reqwest_error)?;

        classify_response(response, "discord").await
    }
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
            title: "Deadline".to_string(),
            message: "Clause 4.2".to_string(),
            due_date: None,
            reminder_type: "on_deadline".to_string(),
            channel_type: ChannelType::Discord,
        }
    }

    #[tokio::test]
    async fn unset_url_reports_not_configured() {
        let ch = DiscordChannel::new(Client::new(), None);
        let err = ch.send(&n()).await.unwrap_err();
        assert_eq!(err.safe_category(), "channel_not_configured");
    }

    #[tokio::test]
    async fn malformed_url_reports_not_configured() {
        let ch = DiscordChannel::new(Client::new(), Some("not a url".into()));
        let err = ch.send(&n()).await.unwrap_err();
        assert_eq!(err.safe_category(), "channel_not_configured");
    }
}
