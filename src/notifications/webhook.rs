//! Generic webhook channel.
//!
//! Sends a structured JSON POST to a user-configured URL. The
//! destination is validated by [`crate::notifications::ssrf`] before
//! every request. Redirects are disabled at the client level (see
//! [`crate::notifications::http`]), so a 30x response is treated as a
//! delivery failure rather than being followed.

use async_trait::async_trait;
use reqwest::Url;
use serde_json::json;

use crate::notifications::channel::{DeliveryError, Notification, NotificationChannel};
use crate::notifications::http::build_pinned_client;
use crate::notifications::ssrf;
use crate::notifications::telegram::{classify_reqwest_error, classify_response};

/// A generic webhook delivery channel.
///
/// Manual `Debug`: the URL may embed credentials in its query
/// string, so it is redacted. `allow_loopback` is shown because it
/// is an operational security flag, not a secret.
pub struct WebhookChannel {
    url: Option<Url>,
    timeout: std::time::Duration,
    /// Test-only escape hatch (see `ssrf::validate`).
    allow_loopback: bool,
}

impl std::fmt::Debug for WebhookChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebhookChannel")
            .field("url", &self.url.as_ref().map(|_| "<redacted>"))
            .field("timeout", &self.timeout)
            .field("allow_loopback", &self.allow_loopback)
            .finish()
    }
}

impl WebhookChannel {
    /// Parses `url`. Malformed URLs are stored as `None` and produce
    /// [`DeliveryError::InvalidUrl`] on send.
    ///
    /// `timeout` applies to the per-request pinned client built inside
    /// [`Self::send`]. `allow_loopback` is a test-only escape hatch;
    /// [`crate::config::Config::load`] refuses to start with it set.
    pub fn new(url: Option<String>, timeout: std::time::Duration, allow_loopback: bool) -> Self {
        let parsed = url.and_then(|s| Url::parse(&s).ok());
        Self {
            url: parsed,
            timeout,
            allow_loopback,
        }
    }
}

#[async_trait]
impl NotificationChannel for WebhookChannel {
    fn name(&self) -> &'static str {
        "webhook"
    }

    async fn send(&self, notification: &Notification) -> Result<(), DeliveryError> {
        let url = self.url.as_ref().ok_or(DeliveryError::NotConfigured)?;

        // SSRF gate runs on every request. It returns the exact set of
        // IP addresses that were validated. The HTTP client is then
        // pinned to those addresses so a second DNS lookup during the
        // connect phase cannot slip an attacker-controlled address in.
        let addrs = ssrf::validate_and_resolve(url, self.allow_loopback)?;
        let host = url.host_str().ok_or(DeliveryError::InvalidUrl)?;
        let client = build_pinned_client(host, &addrs, self.timeout)?;

        let body = json!({
            "event": "contract_reminder_due",
            "reminder_id": notification.reminder_id,
            "contract_id": notification.contract_id,
            "obligation_id": notification.obligation_id,
            "title": notification.title,
            "message": notification.message,
            "due_date": notification.due_date.map(|d| d.to_string()),
            "reminder_type": notification.reminder_type,
        });

        let response = client
            .post(url.clone())
            .header("Idempotency-Key", notification.reminder_id.to_string())
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(classify_reqwest_error)?;

        classify_response(response, "webhook").await
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
            message: "Clause".to_string(),
            due_date: None,
            reminder_type: "on_deadline".to_string(),
            channel_type: ChannelType::Webhook,
        }
    }

    #[tokio::test]
    async fn unset_url_reports_not_configured() {
        let ch = WebhookChannel::new(None, std::time::Duration::from_secs(5), false);
        let err = ch.send(&n()).await.unwrap_err();
        assert_eq!(err.safe_category(), "channel_not_configured");
    }

    #[tokio::test]
    async fn loopback_url_blocked_by_default() {
        let ch = WebhookChannel::new(
            Some("https://127.0.0.1:9999/hook".into()),
            std::time::Duration::from_secs(5),
            false,
        );
        let err = ch.send(&n()).await.unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    #[tokio::test]
    async fn http_scheme_blocked() {
        let ch = WebhookChannel::new(Some("http://example.com/hook".into()), std::time::Duration::from_secs(5), false);
        let err = ch.send(&n()).await.unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    /// Proves that the pinned client connects only to the validated
    /// address and does **not** perform its own DNS lookup.
    ///
    /// The hostname used here (`.invalid` per RFC 6761) is guaranteed
    /// to never resolve. If reqwest were allowed to resolve it, the
    /// request would fail. Because we pin the resolver to the mock
    /// server's loopback address, the request reaches the mock.
    #[tokio::test]
    async fn pinned_client_uses_only_validated_addresses() {
        use std::net::SocketAddr;
        use std::time::Duration;

        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        // Extract the mock server's SocketAddr from its URI.
        let uri = server.uri();
        let addr: SocketAddr = uri
            .trim_start_matches("http://")
            .parse()
            .expect("wiremock uri parses to SocketAddr");

        // A hostname that will never resolve. Reaching the mock server
        // through this name proves the pin is authoritative.
        let fake_host = "pinned-client.invalid";
        let client = build_pinned_client(fake_host, &[addr], Duration::from_secs(5))
            .expect("pinned client builds");

        let url = format!("http://{}:{}/probe", fake_host, addr.port());
        let resp = client
            .get(&url)
            .send()
            .await
            .expect("pinned client should reach the mock server");

        assert_eq!(resp.status(), 200);
    }

    /// Documents the flip side: without a pin, the same `.invalid`
    /// hostname cannot be resolved, so no request is sent.
    #[tokio::test]
    async fn unpinned_client_cannot_reach_invalid_hostname() {
        use std::time::Duration;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .expect("plain client builds");

        let result = client.get("http://pinned-client.invalid:9/probe").send().await;
        assert!(
            result.is_err(),
            "a plain reqwest client must fail to resolve `.invalid` — this is why pinning is required"
        );
    }
}
