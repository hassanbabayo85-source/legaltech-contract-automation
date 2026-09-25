//! Typed configuration for each notification channel.
//!
//! User-supplied configuration is parsed into one of these variants,
//! validated, then serialized to JSON and encrypted before storage.
//! The raw configuration never leaves the server; API DTOs expose
//! only safe metadata.
//!
//! Every variant uses `#[serde(deny_unknown_fields)]` so a client
//! cannot smuggle in extra keys (e.g. arbitrary HTTP headers).

use serde::{Deserialize, Serialize};
use url::Url;

use crate::reminders::types::ChannelType;

/// The union of all channel configurations.
///
/// Tagged by `channel_type`, which must match the row's
/// `notification_channels.channel_type` column. Mismatches are
/// rejected at insert/update time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "channel_type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChannelConfig {
    Telegram(TelegramConfig),
    Discord(DiscordConfig),
    Webhook(WebhookConfig),
}

impl ChannelConfig {
    pub fn channel_type(&self) -> ChannelType {
        match self {
            ChannelConfig::Telegram(_) => ChannelType::Telegram,
            ChannelConfig::Discord(_) => ChannelType::Discord,
            ChannelConfig::Webhook(_) => ChannelType::Webhook,
        }
    }

    /// Validates the configuration. Returns a short, safe message on
    /// failure. Never includes the offending value.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            ChannelConfig::Telegram(c) => c.validate(),
            ChannelConfig::Discord(c) => c.validate(),
            ChannelConfig::Webhook(c) => c.validate(),
        }
    }
}

// -------------------------------------------------------------------------
// Telegram
// -------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: String,
}

impl TelegramConfig {
    fn validate(&self) -> Result<(), String> {
        let token = self.bot_token.trim();
        if token.is_empty() {
            return Err("telegram.bot_token must not be empty".to_string());
        }
        if token.len() > 200 {
            return Err("telegram.bot_token is too long".to_string());
        }
        // Telegram bot tokens look like `<digits>:<base64-ish>`. We do
        // not attempt to fully validate — the provider will — but we
        // reject the obviously wrong shape so a UI typo fails fast.
        let (prefix, suffix) = token
            .split_once(':')
            .ok_or_else(|| "telegram.bot_token must contain `:`".to_string())?;
        if prefix.is_empty() || !prefix.chars().all(|c| c.is_ascii_digit()) {
            return Err("telegram.bot_token has an invalid shape".to_string());
        }
        if suffix.is_empty() || suffix.len() > 100 {
            return Err("telegram.bot_token has an invalid shape".to_string());
        }

        let chat = self.chat_id.trim();
        if chat.is_empty() {
            return Err("telegram.chat_id must not be empty".to_string());
        }
        if chat.len() > 100 {
            return Err("telegram.chat_id is too long".to_string());
        }
        // Chat ids are integers (possibly negative for groups) or
        // `@channelname` public identifiers.
        let ok_int = chat.parse::<i64>().is_ok();
        let ok_at = chat.starts_with('@') && chat.len() > 1;
        if !ok_int && !ok_at {
            return Err("telegram.chat_id is not a valid id or @username".to_string());
        }

        Ok(())
    }
}

// -------------------------------------------------------------------------
// Discord
// -------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscordConfig {
    pub webhook_url: String,
}

impl DiscordConfig {
    fn validate(&self) -> Result<(), String> {
        let raw = self.webhook_url.trim();
        if raw.is_empty() {
            return Err("discord.webhook_url must not be empty".to_string());
        }
        if raw.len() > 500 {
            return Err("discord.webhook_url is too long".to_string());
        }
        let url = Url::parse(raw).map_err(|_| "discord.webhook_url is not a URL".to_string())?;
        if url.scheme() != "https" {
            return Err("discord.webhook_url must use https".to_string());
        }
        let host = url.host_str().unwrap_or("");
        if !(host == "discord.com" || host == "discordapp.com" || host.ends_with(".discord.com")) {
            return Err("discord.webhook_url must point at discord.com".to_string());
        }
        if !url.path().starts_with("/api/webhooks/") {
            return Err("discord.webhook_url is not a webhook URL".to_string());
        }
        Ok(())
    }
}

// -------------------------------------------------------------------------
// Generic webhook
// -------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookConfig {
    pub url: String,
    #[serde(default)]
    pub auth: Option<WebhookAuth>,
}

/// The only supported outbound authentication mechanisms.
///
/// No arbitrary header forwarding is permitted — that would let a
/// client inject `Host`, `Cookie`, `X-Forwarded-For`, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WebhookAuth {
    /// `Authorization: Bearer <token>`.
    Bearer { token: String },
    /// `X-LexHack-Signature: <hex hmac-sha256(body, secret)>` — the
    /// receiver can verify the payload came from us.
    Hmac { secret: String },
}

impl WebhookConfig {
    fn validate(&self) -> Result<(), String> {
        let raw = self.url.trim();
        if raw.is_empty() {
            return Err("webhook.url must not be empty".to_string());
        }
        if raw.len() > 2000 {
            return Err("webhook.url is too long".to_string());
        }
        let url = Url::parse(raw).map_err(|_| "webhook.url is not a URL".to_string())?;
        if url.scheme() != "https" {
            return Err("webhook.url must use https".to_string());
        }
        if let Some(auth) = &self.auth {
            match auth {
                WebhookAuth::Bearer { token } => {
                    if token.trim().is_empty() {
                        return Err("webhook.auth.token must not be empty".to_string());
                    }
                    if token.len() > 1024 {
                        return Err("webhook.auth.token is too long".to_string());
                    }
                }
                WebhookAuth::Hmac { secret } => {
                    if secret.trim().is_empty() {
                        return Err("webhook.auth.secret must not be empty".to_string());
                    }
                    if secret.len() > 1024 {
                        return Err("webhook.auth.secret is too long".to_string());
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telegram_accepts_valid_config() {
        let c = ChannelConfig::Telegram(TelegramConfig {
            bot_token: "123456:ABC-DEF1234".to_string(),
            chat_id: "-1001234567890".to_string(),
        });
        assert!(c.validate().is_ok());
    }

    #[test]
    fn telegram_accepts_at_username() {
        let c = ChannelConfig::Telegram(TelegramConfig {
            bot_token: "123456:ABC".to_string(),
            chat_id: "@mychannel".to_string(),
        });
        assert!(c.validate().is_ok());
    }

    #[test]
    fn telegram_rejects_missing_colon_in_token() {
        let c = ChannelConfig::Telegram(TelegramConfig {
            bot_token: "abc".to_string(),
            chat_id: "-100".to_string(),
        });
        assert!(c.validate().is_err());
    }

    #[test]
    fn telegram_rejects_non_numeric_prefix() {
        let c = ChannelConfig::Telegram(TelegramConfig {
            bot_token: "abc:def".to_string(),
            chat_id: "-100".to_string(),
        });
        assert!(c.validate().is_err());
    }

    #[test]
    fn discord_accepts_valid_webhook() {
        let c = ChannelConfig::Discord(DiscordConfig {
            webhook_url: "https://discord.com/api/webhooks/123/abc".to_string(),
        });
        assert!(c.validate().is_ok());
    }

    #[test]
    fn discord_rejects_non_discord_host() {
        let c = ChannelConfig::Discord(DiscordConfig {
            webhook_url: "https://example.com/api/webhooks/1/x".to_string(),
        });
        assert!(c.validate().is_err());
    }

    #[test]
    fn discord_rejects_http() {
        let c = ChannelConfig::Discord(DiscordConfig {
            webhook_url: "http://discord.com/api/webhooks/1/x".to_string(),
        });
        assert!(c.validate().is_err());
    }

    #[test]
    fn webhook_accepts_valid_url() {
        let c = ChannelConfig::Webhook(WebhookConfig {
            url: "https://example.com/hook".to_string(),
            auth: None,
        });
        assert!(c.validate().is_ok());
    }

    #[test]
    fn webhook_rejects_http() {
        let c = ChannelConfig::Webhook(WebhookConfig {
            url: "http://example.com/hook".to_string(),
            auth: None,
        });
        assert!(c.validate().is_err());
    }

    #[test]
    fn webhook_accepts_bearer_auth() {
        let c = ChannelConfig::Webhook(WebhookConfig {
            url: "https://example.com/hook".to_string(),
            auth: Some(WebhookAuth::Bearer {
                token: "secret".to_string(),
            }),
        });
        assert!(c.validate().is_ok());
    }

    #[test]
    fn webhook_rejects_empty_bearer_token() {
        let c = ChannelConfig::Webhook(WebhookConfig {
            url: "https://example.com/hook".to_string(),
            auth: Some(WebhookAuth::Bearer {
                token: "   ".to_string(),
            }),
        });
        assert!(c.validate().is_err());
    }

    #[test]
    fn serde_rejects_unknown_fields() {
        let json = r#"{
            "channel_type": "telegram",
            "bot_token": "1:a",
            "chat_id": "-1",
            "extra": "nope"
        }"#;
        let result: Result<ChannelConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }
}
