//! Application configuration.
//!
//! Configuration is loaded exclusively from environment variables
//! (optionally populated from a local `.env` file during development
//! via `dotenvy`). Nothing here hard-codes secrets, and nothing here
//! ever logs a secret value.

use std::env;
use std::fmt;

pub const DEFAULT_SESSION_LIFETIME_SECONDS: i64 = 7 * 24 * 60 * 60;
pub const DEFAULT_ARGON2_M_COST: u32 = 19_456;
pub const DEFAULT_ARGON2_T_COST: u32 = 2;
pub const DEFAULT_ARGON2_P_COST: u32 = 1;
pub const DEFAULT_RATE_LIMIT_LOGIN_PER_MINUTE: u32 = 5;
pub const DEFAULT_RATE_LIMIT_REGISTER_PER_HOUR: u32 = 3;
pub const DEFAULT_AI_BASE_URL: &str = "https://api.openai.com";
pub const DEFAULT_AI_MODEL: &str = "gpt-4o-mini";
pub const DEFAULT_AI_TIMEOUT_SECONDS: u64 = 60;
pub const DEFAULT_AI_MAX_INPUT_CHARS: usize = 100_000;
pub const DEFAULT_AI_MAX_RETRIES: u32 = 2;

pub const DEFAULT_WORKER_ENABLED: bool = true;
pub const DEFAULT_WORKER_POLL_INTERVAL_SECONDS: u64 = 10;
pub const DEFAULT_WORKER_BATCH_SIZE: u32 = 50;
pub const DEFAULT_WORKER_MAX_CONCURRENCY: u32 = 8;
pub const DEFAULT_WORKER_CLAIM_TIMEOUT_SECONDS: u64 = 300;
pub const DEFAULT_WORKER_SHUTDOWN_TIMEOUT_SECONDS: u64 = 30;
pub const DEFAULT_NOTIFICATION_REQUEST_TIMEOUT_SECONDS: u64 = 15;
pub const DEFAULT_NOTIFICATION_MAX_RETRIES: u32 = 5;

// PART 08 — per-user rate limits.
pub const DEFAULT_RATE_LIMIT_ANALYZE_PER_HOUR: u32 = 10;
pub const DEFAULT_RATE_LIMIT_CHANNEL_TEST_PER_MINUTE: u32 = 5;
pub const DEFAULT_RATE_LIMIT_CHANNEL_MUTATION_PER_HOUR: u32 = 20;

#[derive(Clone)]
pub struct Config {
    database_url: String,
    pub host: String,
    pub port: u16,
    pub db_max_connections: u32,
    pub session_lifetime_seconds: i64,
    pub argon2_m_cost: u32,
    pub argon2_t_cost: u32,
    pub argon2_p_cost: u32,
    pub rate_limit_login_per_minute: u32,
    pub rate_limit_register_per_hour: u32,
    pub rate_limit_analyze_per_hour: u32,
    pub rate_limit_channel_test_per_minute: u32,
    pub rate_limit_channel_mutation_per_hour: u32,
    pub ai_base_url: String,
    pub ai_api_key: Option<String>,
    pub ai_model: String,
    pub ai_timeout_seconds: u64,
    pub ai_max_input_chars: usize,
    pub ai_max_retries: u32,
    pub worker_enabled: bool,
    pub worker_poll_interval_seconds: u64,
    pub worker_batch_size: u32,
    pub worker_max_concurrency: u32,
    pub worker_claim_timeout_seconds: u64,
    pub worker_shutdown_timeout_seconds: u64,
    pub notification_request_timeout_seconds: u64,
    pub notification_max_retries: u32,
    /// Base64-encoded 32-byte key for notification-channel encryption.
    ///
    /// Required in production (`Config::load` refuses to return without
    /// it). Optional in `from_lookup` so unit tests can build a
    /// `Config` without providing one.
    pub notification_secret_key: Option<String>,
    pub telegram_bot_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub discord_webhook_url: Option<String>,
    pub webhook_url: Option<String>,
    pub webhook_allow_loopback: bool,
    /// Comma-separated list of origins allowed by CORS.
    ///
    /// Defaults to the Vite/Trunk dev origins (`http://localhost:8080`,
    /// `http://127.0.0.1:8080`) so `trunk serve` works out of the box.
    /// Production deployments MUST set this explicitly. Wildcards are
    /// not permitted — every entry must be an exact scheme+host+port.
    pub cors_allowed_origins: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    Missing(&'static str),

    #[error("invalid value for environment variable {name}: {reason}")]
    Invalid { name: &'static str, reason: String },
}

/// Default CORS origins for development.
///
/// These match `trunk serve` (which listens on `:8080`) and the common
/// Vite dev server. Production deployments should override with an
/// explicit list.
fn default_cors_origins() -> Vec<String> {
    vec![
        "http://localhost:8080".to_string(),
        "http://127.0.0.1:8080".to_string(),
    ]
}

impl Config {
    /// Loads configuration from the process environment.
    ///
    /// Unlike [`Config::from_lookup`], this production entry point
    /// requires `NOTIFICATION_SECRET_KEY` to be set. A deployment
    /// that forgets the key must not silently fall back to a state
    /// where channel secrets cannot be stored securely.
    pub fn load() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();
        let config = Self::from_lookup(|key| env::var(key).ok())?;
        if config.notification_secret_key.is_none() {
            return Err(ConfigError::Missing("NOTIFICATION_SECRET_KEY"));
        }
        // Test-only escape hatch. Refusing to start in production is
        // stronger than documenting "never enable this": a stray env
        // variable cannot weaken SSRF protection.
        if config.webhook_allow_loopback {
            return Err(ConfigError::Invalid {
                name: "WEBHOOK_ALLOW_LOOPBACK",
                reason: "must not be true in production; this is a test-only escape hatch"
                    .to_string(),
            });
        }
        Ok(config)
    }

    pub fn from_lookup<F>(lookup: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let database_url = lookup("DATABASE_URL").ok_or(ConfigError::Missing("DATABASE_URL"))?;
        if database_url.trim().is_empty() {
            return Err(ConfigError::Invalid {
                name: "DATABASE_URL",
                reason: "must not be empty".to_string(),
            });
        }

        let host = lookup("HOST")
            .map(|v| v.trim().to_string())
            .unwrap_or_else(|| "0.0.0.0".to_string());
        if host.is_empty() {
            return Err(ConfigError::Invalid {
                name: "HOST",
                reason: "must not be empty".to_string(),
            });
        }

        let port = match lookup("PORT") {
            Some(raw) => raw.parse::<u16>().map_err(|_| ConfigError::Invalid {
                name: "PORT",
                reason: format!("`{raw}` is not a valid u16 port number"),
            })?,
            None => 3000,
        };
        if port == 0 {
            return Err(ConfigError::Invalid {
                name: "PORT",
                reason: "must be greater than 0".to_string(),
            });
        }

        let db_max_connections = parse_u32(&lookup, "DATABASE_MAX_CONNECTIONS", 5)?;
        if db_max_connections == 0 {
            return Err(ConfigError::Invalid {
                name: "DATABASE_MAX_CONNECTIONS",
                reason: "must be greater than 0".to_string(),
            });
        }

        let session_lifetime_seconds = parse_i64(
            &lookup,
            "SESSION_LIFETIME_SECONDS",
            DEFAULT_SESSION_LIFETIME_SECONDS,
        )?;
        if session_lifetime_seconds <= 0 {
            return Err(ConfigError::Invalid {
                name: "SESSION_LIFETIME_SECONDS",
                reason: "must be greater than 0".to_string(),
            });
        }

        let argon2_m_cost = parse_u32(&lookup, "ARGON2_M_COST", DEFAULT_ARGON2_M_COST)?;
        if !(8..=1_048_576).contains(&argon2_m_cost) {
            return Err(ConfigError::Invalid {
                name: "ARGON2_M_COST",
                reason: "must be between 8 and 1048576 KiB".to_string(),
            });
        }

        let argon2_t_cost = parse_u32(&lookup, "ARGON2_T_COST", DEFAULT_ARGON2_T_COST)?;
        if !(1..=100).contains(&argon2_t_cost) {
            return Err(ConfigError::Invalid {
                name: "ARGON2_T_COST",
                reason: "must be between 1 and 100".to_string(),
            });
        }

        let argon2_p_cost = parse_u32(&lookup, "ARGON2_P_COST", DEFAULT_ARGON2_P_COST)?;
        if !(1..=64).contains(&argon2_p_cost) {
            return Err(ConfigError::Invalid {
                name: "ARGON2_P_COST",
                reason: "must be between 1 and 64".to_string(),
            });
        }

        let rate_limit_login_per_minute = parse_u32(
            &lookup,
            "RATE_LIMIT_LOGIN_PER_MINUTE",
            DEFAULT_RATE_LIMIT_LOGIN_PER_MINUTE,
        )?;
        if rate_limit_login_per_minute == 0 {
            return Err(ConfigError::Invalid {
                name: "RATE_LIMIT_LOGIN_PER_MINUTE",
                reason: "must be greater than 0".to_string(),
            });
        }

        let rate_limit_register_per_hour = parse_u32(
            &lookup,
            "RATE_LIMIT_REGISTER_PER_HOUR",
            DEFAULT_RATE_LIMIT_REGISTER_PER_HOUR,
        )?;
        if rate_limit_register_per_hour == 0 {
            return Err(ConfigError::Invalid {
                name: "RATE_LIMIT_REGISTER_PER_HOUR",
                reason: "must be greater than 0".to_string(),
            });
        }

        let rate_limit_analyze_per_hour = parse_u32(
            &lookup,
            "RATE_LIMIT_ANALYZE_PER_HOUR",
            DEFAULT_RATE_LIMIT_ANALYZE_PER_HOUR,
        )?;
        if rate_limit_analyze_per_hour == 0 {
            return Err(ConfigError::Invalid {
                name: "RATE_LIMIT_ANALYZE_PER_HOUR",
                reason: "must be greater than 0".to_string(),
            });
        }

        let rate_limit_channel_test_per_minute = parse_u32(
            &lookup,
            "RATE_LIMIT_CHANNEL_TEST_PER_MINUTE",
            DEFAULT_RATE_LIMIT_CHANNEL_TEST_PER_MINUTE,
        )?;
        if rate_limit_channel_test_per_minute == 0 {
            return Err(ConfigError::Invalid {
                name: "RATE_LIMIT_CHANNEL_TEST_PER_MINUTE",
                reason: "must be greater than 0".to_string(),
            });
        }

        let rate_limit_channel_mutation_per_hour = parse_u32(
            &lookup,
            "RATE_LIMIT_CHANNEL_MUTATION_PER_HOUR",
            DEFAULT_RATE_LIMIT_CHANNEL_MUTATION_PER_HOUR,
        )?;
        if rate_limit_channel_mutation_per_hour == 0 {
            return Err(ConfigError::Invalid {
                name: "RATE_LIMIT_CHANNEL_MUTATION_PER_HOUR",
                reason: "must be greater than 0".to_string(),
            });
        }

        let ai_base_url = lookup("AI_BASE_URL")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_AI_BASE_URL.to_string());
        let ai_api_key = lookup("AI_API_KEY")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let ai_model = lookup("AI_MODEL")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_AI_MODEL.to_string());
        let ai_timeout_seconds =
            parse_u64(&lookup, "AI_TIMEOUT_SECONDS", DEFAULT_AI_TIMEOUT_SECONDS)?;
        if ai_timeout_seconds == 0 {
            return Err(ConfigError::Invalid {
                name: "AI_TIMEOUT_SECONDS",
                reason: "must be greater than 0".to_string(),
            });
        }
        let ai_max_input_chars =
            parse_usize(&lookup, "AI_MAX_INPUT_CHARS", DEFAULT_AI_MAX_INPUT_CHARS)?;
        if ai_max_input_chars == 0 {
            return Err(ConfigError::Invalid {
                name: "AI_MAX_INPUT_CHARS",
                reason: "must be greater than 0".to_string(),
            });
        }
        let ai_max_retries = parse_u32(&lookup, "AI_MAX_RETRIES", DEFAULT_AI_MAX_RETRIES)?;
        if ai_max_retries > 10 {
            return Err(ConfigError::Invalid {
                name: "AI_MAX_RETRIES",
                reason: "must be at most 10".to_string(),
            });
        }

        let worker_enabled = parse_bool(&lookup, "WORKER_ENABLED", DEFAULT_WORKER_ENABLED)?;
        let worker_poll_interval_seconds = parse_u64(
            &lookup,
            "WORKER_POLL_INTERVAL_SECONDS",
            DEFAULT_WORKER_POLL_INTERVAL_SECONDS,
        )?;
        if worker_poll_interval_seconds == 0 {
            return Err(ConfigError::Invalid {
                name: "WORKER_POLL_INTERVAL_SECONDS",
                reason: "must be greater than 0".to_string(),
            });
        }
        let worker_batch_size = parse_u32(&lookup, "WORKER_BATCH_SIZE", DEFAULT_WORKER_BATCH_SIZE)?;
        if worker_batch_size == 0 || worker_batch_size > 10_000 {
            return Err(ConfigError::Invalid {
                name: "WORKER_BATCH_SIZE",
                reason: "must be between 1 and 10000".to_string(),
            });
        }
        let worker_max_concurrency = parse_u32(
            &lookup,
            "WORKER_MAX_CONCURRENCY",
            DEFAULT_WORKER_MAX_CONCURRENCY,
        )?;
        if worker_max_concurrency == 0 || worker_max_concurrency > 1024 {
            return Err(ConfigError::Invalid {
                name: "WORKER_MAX_CONCURRENCY",
                reason: "must be between 1 and 1024".to_string(),
            });
        }
        let worker_claim_timeout_seconds = parse_u64(
            &lookup,
            "WORKER_CLAIM_TIMEOUT_SECONDS",
            DEFAULT_WORKER_CLAIM_TIMEOUT_SECONDS,
        )?;
        if worker_claim_timeout_seconds == 0 {
            return Err(ConfigError::Invalid {
                name: "WORKER_CLAIM_TIMEOUT_SECONDS",
                reason: "must be greater than 0".to_string(),
            });
        }
        let worker_shutdown_timeout_seconds = parse_u64(
            &lookup,
            "WORKER_SHUTDOWN_TIMEOUT_SECONDS",
            DEFAULT_WORKER_SHUTDOWN_TIMEOUT_SECONDS,
        )?;
        if worker_shutdown_timeout_seconds == 0 {
            return Err(ConfigError::Invalid {
                name: "WORKER_SHUTDOWN_TIMEOUT_SECONDS",
                reason: "must be greater than 0".to_string(),
            });
        }

        let notification_request_timeout_seconds = parse_u64(
            &lookup,
            "NOTIFICATION_REQUEST_TIMEOUT_SECONDS",
            DEFAULT_NOTIFICATION_REQUEST_TIMEOUT_SECONDS,
        )?;
        if notification_request_timeout_seconds == 0 {
            return Err(ConfigError::Invalid {
                name: "NOTIFICATION_REQUEST_TIMEOUT_SECONDS",
                reason: "must be greater than 0".to_string(),
            });
        }
        let notification_max_retries = parse_u32(
            &lookup,
            "NOTIFICATION_MAX_RETRIES",
            DEFAULT_NOTIFICATION_MAX_RETRIES,
        )?;
        if notification_max_retries > 20 {
            return Err(ConfigError::Invalid {
                name: "NOTIFICATION_MAX_RETRIES",
                reason: "must be at most 20".to_string(),
            });
        }

        let notification_secret_key = lookup("NOTIFICATION_SECRET_KEY")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());

        let telegram_bot_token = lookup("TELEGRAM_BOT_TOKEN")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let telegram_chat_id = lookup("TELEGRAM_CHAT_ID")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        if telegram_bot_token.is_some() && telegram_chat_id.is_none() {
            return Err(ConfigError::Invalid {
                name: "TELEGRAM_CHAT_ID",
                reason: "required when TELEGRAM_BOT_TOKEN is set".to_string(),
            });
        }

        let discord_webhook_url = lookup("DISCORD_WEBHOOK_URL")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let webhook_url = lookup("WEBHOOK_URL")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let webhook_allow_loopback = parse_bool(&lookup, "WEBHOOK_ALLOW_LOOPBACK", false)?;

        let cors_allowed_origins = lookup("CORS_ALLOWED_ORIGINS")
            .map(|raw| {
                raw.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(default_cors_origins);

        if cors_allowed_origins.iter().any(|o| o == "*") {
            return Err(ConfigError::Invalid {
                name: "CORS_ALLOWED_ORIGINS",
                reason: "wildcard `*` is not allowed".to_string(),
            });
        }

        Ok(Config {
            database_url,
            host,
            port,
            db_max_connections,
            session_lifetime_seconds,
            argon2_m_cost,
            argon2_t_cost,
            argon2_p_cost,
            rate_limit_login_per_minute,
            rate_limit_register_per_hour,
            rate_limit_analyze_per_hour,
            rate_limit_channel_test_per_minute,
            rate_limit_channel_mutation_per_hour,
            ai_base_url,
            ai_api_key,
            ai_model,
            ai_timeout_seconds,
            ai_max_input_chars,
            ai_max_retries,
            worker_enabled,
            worker_poll_interval_seconds,
            worker_batch_size,
            worker_max_concurrency,
            worker_claim_timeout_seconds,
            worker_shutdown_timeout_seconds,
            notification_request_timeout_seconds,
            notification_max_retries,
            notification_secret_key,
            telegram_bot_token,
            telegram_chat_id,
            discord_webhook_url,
            webhook_url,
            webhook_allow_loopback,
            cors_allowed_origins,
        })
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn socket_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

fn parse_i64<F>(lookup: &F, name: &'static str, default: i64) -> Result<i64, ConfigError>
where
    F: Fn(&str) -> Option<String>,
{
    match lookup(name) {
        Some(raw) => raw.parse::<i64>().map_err(|_| ConfigError::Invalid {
            name,
            reason: format!("`{raw}` is not a valid i64"),
        }),
        None => Ok(default),
    }
}

fn parse_u64<F>(lookup: &F, name: &'static str, default: u64) -> Result<u64, ConfigError>
where
    F: Fn(&str) -> Option<String>,
{
    match lookup(name) {
        Some(raw) => raw.parse::<u64>().map_err(|_| ConfigError::Invalid {
            name,
            reason: format!("`{raw}` is not a valid u64"),
        }),
        None => Ok(default),
    }
}

fn parse_u32<F>(lookup: &F, name: &'static str, default: u32) -> Result<u32, ConfigError>
where
    F: Fn(&str) -> Option<String>,
{
    match lookup(name) {
        Some(raw) => raw.parse::<u32>().map_err(|_| ConfigError::Invalid {
            name,
            reason: format!("`{raw}` is not a valid u32"),
        }),
        None => Ok(default),
    }
}

fn parse_usize<F>(lookup: &F, name: &'static str, default: usize) -> Result<usize, ConfigError>
where
    F: Fn(&str) -> Option<String>,
{
    match lookup(name) {
        Some(raw) => raw.parse::<usize>().map_err(|_| ConfigError::Invalid {
            name,
            reason: format!("`{raw}` is not a valid usize"),
        }),
        None => Ok(default),
    }
}

fn parse_bool<F>(lookup: &F, name: &'static str, default: bool) -> Result<bool, ConfigError>
where
    F: Fn(&str) -> Option<String>,
{
    match lookup(name).map(|v| v.trim().to_ascii_lowercase()) {
        None => Ok(default),
        Some(v) => match v.as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(ConfigError::Invalid {
                name,
                reason: format!("`{v}` is not a boolean (use 1/0, true/false)"),
            }),
        },
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"<redacted>")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("db_max_connections", &self.db_max_connections)
            .field("session_lifetime_seconds", &self.session_lifetime_seconds)
            .field("argon2_m_cost", &self.argon2_m_cost)
            .field("argon2_t_cost", &self.argon2_t_cost)
            .field("argon2_p_cost", &self.argon2_p_cost)
            .field(
                "rate_limit_login_per_minute",
                &self.rate_limit_login_per_minute,
            )
            .field(
                "rate_limit_register_per_hour",
                &self.rate_limit_register_per_hour,
            )
            .field(
                "rate_limit_analyze_per_hour",
                &self.rate_limit_analyze_per_hour,
            )
            .field(
                "rate_limit_channel_test_per_minute",
                &self.rate_limit_channel_test_per_minute,
            )
            .field(
                "rate_limit_channel_mutation_per_hour",
                &self.rate_limit_channel_mutation_per_hour,
            )
            .field("ai_base_url", &self.ai_base_url)
            .field(
                "ai_api_key",
                &self.ai_api_key.as_ref().map(|_| "<redacted>"),
            )
            .field("ai_model", &self.ai_model)
            .field("ai_timeout_seconds", &self.ai_timeout_seconds)
            .field("ai_max_input_chars", &self.ai_max_input_chars)
            .field("ai_max_retries", &self.ai_max_retries)
            .field("worker_enabled", &self.worker_enabled)
            .field(
                "worker_poll_interval_seconds",
                &self.worker_poll_interval_seconds,
            )
            .field("worker_batch_size", &self.worker_batch_size)
            .field("worker_max_concurrency", &self.worker_max_concurrency)
            .field(
                "worker_claim_timeout_seconds",
                &self.worker_claim_timeout_seconds,
            )
            .field(
                "worker_shutdown_timeout_seconds",
                &self.worker_shutdown_timeout_seconds,
            )
            .field(
                "notification_request_timeout_seconds",
                &self.notification_request_timeout_seconds,
            )
            .field("notification_max_retries", &self.notification_max_retries)
            .field(
                "notification_secret_key",
                &self.notification_secret_key.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "telegram_bot_token",
                &self.telegram_bot_token.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "telegram_chat_id",
                &self.telegram_chat_id.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "discord_webhook_url",
                &self.discord_webhook_url.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "webhook_url",
                &self.webhook_url.as_ref().map(|_| "<redacted>"),
            )
            .field("webhook_allow_loopback", &self.webhook_allow_loopback)
            .field("cors_allowed_origins", &self.cors_allowed_origins)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn load_from(pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let map: HashMap<&str, &str> = pairs.iter().copied().collect();
        Config::from_lookup(|key| map.get(key).map(|value| (*value).to_string()))
    }

    fn minimal_valid() -> Vec<(&'static str, &'static str)> {
        vec![("DATABASE_URL", "postgres://user:pass@localhost/db")]
    }

    #[test]
    fn fails_when_database_url_missing() {
        assert!(matches!(
            load_from(&[]),
            Err(ConfigError::Missing("DATABASE_URL"))
        ));
    }

    #[test]
    fn applies_defaults_for_worker() {
        let c = load_from(&minimal_valid()).unwrap();
        assert!(c.worker_enabled);
        assert_eq!(
            c.worker_poll_interval_seconds,
            DEFAULT_WORKER_POLL_INTERVAL_SECONDS
        );
        assert_eq!(c.worker_batch_size, DEFAULT_WORKER_BATCH_SIZE);
        assert_eq!(c.worker_max_concurrency, DEFAULT_WORKER_MAX_CONCURRENCY);
        assert_eq!(
            c.worker_claim_timeout_seconds,
            DEFAULT_WORKER_CLAIM_TIMEOUT_SECONDS
        );
        assert_eq!(
            c.worker_shutdown_timeout_seconds,
            DEFAULT_WORKER_SHUTDOWN_TIMEOUT_SECONDS
        );
        assert_eq!(
            c.notification_request_timeout_seconds,
            DEFAULT_NOTIFICATION_REQUEST_TIMEOUT_SECONDS
        );
        assert_eq!(c.notification_max_retries, DEFAULT_NOTIFICATION_MAX_RETRIES);
        assert!(c.telegram_bot_token.is_none());
        assert!(c.discord_webhook_url.is_none());
        assert!(c.webhook_url.is_none());
        assert!(!c.webhook_allow_loopback);
    }

    #[test]
    fn applies_defaults_for_channel_rate_limits() {
        let c = load_from(&minimal_valid()).unwrap();
        assert_eq!(
            c.rate_limit_analyze_per_hour,
            DEFAULT_RATE_LIMIT_ANALYZE_PER_HOUR
        );
        assert_eq!(
            c.rate_limit_channel_test_per_minute,
            DEFAULT_RATE_LIMIT_CHANNEL_TEST_PER_MINUTE
        );
        assert_eq!(
            c.rate_limit_channel_mutation_per_hour,
            DEFAULT_RATE_LIMIT_CHANNEL_MUTATION_PER_HOUR
        );
    }

    #[test]
    fn notification_secret_key_is_optional_in_from_lookup() {
        let c = load_from(&minimal_valid()).unwrap();
        assert!(c.notification_secret_key.is_none());
    }

    #[test]
    fn notification_secret_key_parses_when_set() {
        let mut p = minimal_valid();
        p.push((
            "NOTIFICATION_SECRET_KEY",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        ));
        let c = load_from(&p).unwrap();
        assert!(c.notification_secret_key.is_some());
    }

    #[test]
    fn rejects_zero_analyze_rate_limit() {
        let mut p = minimal_valid();
        p.push(("RATE_LIMIT_ANALYZE_PER_HOUR", "0"));
        assert!(matches!(
            load_from(&p),
            Err(ConfigError::Invalid {
                name: "RATE_LIMIT_ANALYZE_PER_HOUR",
                ..
            })
        ));
    }

    #[test]
    fn rejects_zero_channel_test_rate_limit() {
        let mut p = minimal_valid();
        p.push(("RATE_LIMIT_CHANNEL_TEST_PER_MINUTE", "0"));
        assert!(matches!(
            load_from(&p),
            Err(ConfigError::Invalid {
                name: "RATE_LIMIT_CHANNEL_TEST_PER_MINUTE",
                ..
            })
        ));
    }

    #[test]
    fn rejects_zero_channel_mutation_rate_limit() {
        let mut p = minimal_valid();
        p.push(("RATE_LIMIT_CHANNEL_MUTATION_PER_HOUR", "0"));
        assert!(matches!(
            load_from(&p),
            Err(ConfigError::Invalid {
                name: "RATE_LIMIT_CHANNEL_MUTATION_PER_HOUR",
                ..
            })
        ));
    }

    #[test]
    fn rejects_zero_worker_poll_interval() {
        let mut p = minimal_valid();
        p.push(("WORKER_POLL_INTERVAL_SECONDS", "0"));
        assert!(matches!(
            load_from(&p),
            Err(ConfigError::Invalid {
                name: "WORKER_POLL_INTERVAL_SECONDS",
                ..
            })
        ));
    }

    #[test]
    fn rejects_zero_worker_batch_size() {
        let mut p = minimal_valid();
        p.push(("WORKER_BATCH_SIZE", "0"));
        assert!(matches!(
            load_from(&p),
            Err(ConfigError::Invalid {
                name: "WORKER_BATCH_SIZE",
                ..
            })
        ));
    }

    #[test]
    fn rejects_zero_worker_max_concurrency() {
        let mut p = minimal_valid();
        p.push(("WORKER_MAX_CONCURRENCY", "0"));
        assert!(matches!(
            load_from(&p),
            Err(ConfigError::Invalid {
                name: "WORKER_MAX_CONCURRENCY",
                ..
            })
        ));
    }

    #[test]
    fn rejects_excessive_worker_max_concurrency() {
        let mut p = minimal_valid();
        p.push(("WORKER_MAX_CONCURRENCY", "9999"));
        assert!(matches!(
            load_from(&p),
            Err(ConfigError::Invalid {
                name: "WORKER_MAX_CONCURRENCY",
                ..
            })
        ));
    }

    #[test]
    fn rejects_zero_claim_timeout() {
        let mut p = minimal_valid();
        p.push(("WORKER_CLAIM_TIMEOUT_SECONDS", "0"));
        assert!(matches!(
            load_from(&p),
            Err(ConfigError::Invalid {
                name: "WORKER_CLAIM_TIMEOUT_SECONDS",
                ..
            })
        ));
    }

    #[test]
    fn rejects_zero_notification_timeout() {
        let mut p = minimal_valid();
        p.push(("NOTIFICATION_REQUEST_TIMEOUT_SECONDS", "0"));
        assert!(matches!(
            load_from(&p),
            Err(ConfigError::Invalid {
                name: "NOTIFICATION_REQUEST_TIMEOUT_SECONDS",
                ..
            })
        ));
    }

    #[test]
    fn rejects_excessive_notification_max_retries() {
        let mut p = minimal_valid();
        p.push(("NOTIFICATION_MAX_RETRIES", "99"));
        assert!(matches!(
            load_from(&p),
            Err(ConfigError::Invalid {
                name: "NOTIFICATION_MAX_RETRIES",
                ..
            })
        ));
    }

    #[test]
    fn parses_worker_enabled_booleans() {
        for (raw, expected) in [
            ("true", true),
            ("1", true),
            ("yes", true),
            ("on", true),
            ("false", false),
            ("0", false),
            ("no", false),
            ("off", false),
        ] {
            let mut p = minimal_valid();
            p.push(("WORKER_ENABLED", raw));
            assert_eq!(
                load_from(&p).unwrap().worker_enabled,
                expected,
                "raw = {raw}"
            );
        }
    }

    #[test]
    fn rejects_invalid_worker_enabled() {
        let mut p = minimal_valid();
        p.push(("WORKER_ENABLED", "maybe"));
        assert!(matches!(
            load_from(&p),
            Err(ConfigError::Invalid {
                name: "WORKER_ENABLED",
                ..
            })
        ));
    }

    #[test]
    fn telegram_token_without_chat_id_is_rejected() {
        let mut p = minimal_valid();
        p.push(("TELEGRAM_BOT_TOKEN", "123456:ABC-DEF"));
        assert!(matches!(
            load_from(&p),
            Err(ConfigError::Invalid {
                name: "TELEGRAM_CHAT_ID",
                ..
            })
        ));
    }

    #[test]
    fn telegram_token_with_chat_id_is_accepted() {
        let mut p = minimal_valid();
        p.push(("TELEGRAM_BOT_TOKEN", "123456:ABC-DEF"));
        p.push(("TELEGRAM_CHAT_ID", "-1001234567890"));
        let c = load_from(&p).unwrap();
        assert!(c.telegram_bot_token.is_some());
        assert!(c.telegram_chat_id.is_some());
    }

    #[test]
    fn debug_output_redacts_all_secrets() {
        let c = load_from(&[
            (
                "DATABASE_URL",
                "postgres://secret_user:secret_pass@localhost/db",
            ),
            ("AI_API_KEY", "sk-secret-key"),
            (
                "NOTIFICATION_SECRET_KEY",
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            ),
            ("TELEGRAM_BOT_TOKEN", "123456:ABC-SECRET"),
            ("TELEGRAM_CHAT_ID", "-1001234567890"),
            (
                "DISCORD_WEBHOOK_URL",
                "https://discord.com/api/webhooks/123/SECRETTOKEN",
            ),
        ])
        .unwrap();
        let s = format!("{c:?}");
        assert!(!s.contains("secret_pass"));
        assert!(!s.contains("sk-secret-key"));
        assert!(!s.contains("ABC-SECRET"));
        assert!(!s.contains("SECRETTOKEN"));
        assert!(s.contains("<redacted>"));
    }

    #[test]
    fn cors_defaults_to_dev_origins() {
        let c = load_from(&minimal_valid()).unwrap();
        assert!(c.cors_allowed_origins.iter().any(|o| o.contains("8080")));
    }

    #[test]
    fn cors_parses_comma_separated_list() {
        let mut p = minimal_valid();
        p.push((
            "CORS_ALLOWED_ORIGINS",
            "https://app.example.com, https://admin.example.com",
        ));
        let c = load_from(&p).unwrap();
        assert_eq!(c.cors_allowed_origins.len(), 2);
        assert!(c
            .cors_allowed_origins
            .iter()
            .any(|o| o == "https://app.example.com"));
    }

    #[test]
    fn cors_rejects_wildcard() {
        let mut p = minimal_valid();
        p.push(("CORS_ALLOWED_ORIGINS", "*,https://example.com"));
        assert!(matches!(
            load_from(&p),
            Err(ConfigError::Invalid {
                name: "CORS_ALLOWED_ORIGINS",
                ..
            })
        ));
    }

    #[test]
    fn database_url_is_not_empty_after_trim() {
        assert!(matches!(
            load_from(&[("DATABASE_URL", "   ")]),
            Err(ConfigError::Invalid {
                name: "DATABASE_URL",
                ..
            })
        ));
    }
}
