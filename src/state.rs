//! Central application state.

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;

use crate::ai::{openai::OpenAiProvider, AiProvider};
use crate::config::Config;
use crate::middleware::rate_limit::{RateLimiter, UserRateLimiter};
use crate::notifications::secrets::SecretKey;
use crate::worker::poller::ChannelRegistry;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    config: Config,
    db_pool: PgPool,
    secret_key: SecretKey,
    login_rate_limiter: RateLimiter,
    register_rate_limiter: RateLimiter,
    analyze_rate_limiter: UserRateLimiter,
    channel_test_rate_limiter: UserRateLimiter,
    channel_mutation_rate_limiter: UserRateLimiter,
    ai_provider: Arc<dyn AiProvider>,
    channels: Arc<ChannelRegistry>,
}

impl AppState {
    /// Builds `AppState` from a config and pool, deriving the secret
    /// key and channel registry. Panics if the key is missing or
    /// invalid — `Config::load()` guarantees it is present in
    /// production.
    pub fn new(config: Config, db_pool: PgPool) -> Self {
        let key = build_secret_key(&config);
        let provider: Arc<dyn AiProvider> = Arc::new(OpenAiProvider::from_config(&config));
        let channels = Arc::new(build_channel_registry(&config));
        Self::with_dependencies(config, db_pool, key, provider, channels)
    }

    /// Full constructor used by tests that need to inject a provider
    /// and/or channel registry.
    pub fn with_dependencies(
        config: Config,
        db_pool: PgPool,
        secret_key: SecretKey,
        ai_provider: Arc<dyn AiProvider>,
        channels: Arc<ChannelRegistry>,
    ) -> Self {
        let login_rate_limiter =
            RateLimiter::new(config.rate_limit_login_per_minute, Duration::from_secs(60));
        let register_rate_limiter = RateLimiter::new(
            config.rate_limit_register_per_hour,
            Duration::from_secs(3600),
        );
        let analyze_rate_limiter = UserRateLimiter::new(
            config.rate_limit_analyze_per_hour,
            Duration::from_secs(3600),
        );
        let channel_test_rate_limiter = UserRateLimiter::new(
            config.rate_limit_channel_test_per_minute,
            Duration::from_secs(60),
        );
        let channel_mutation_rate_limiter = UserRateLimiter::new(
            config.rate_limit_channel_mutation_per_hour,
            Duration::from_secs(3600),
        );
        Self {
            inner: Arc::new(AppStateInner {
                config,
                db_pool,
                secret_key,
                login_rate_limiter,
                register_rate_limiter,
                analyze_rate_limiter,
                channel_test_rate_limiter,
                channel_mutation_rate_limiter,
                ai_provider,
                channels,
            }),
        }
    }

    /// Convenience constructor preserved for tests from earlier parts.
    pub fn with_provider(
        config: Config,
        db_pool: PgPool,
        ai_provider: Arc<dyn AiProvider>,
        channels: Arc<ChannelRegistry>,
    ) -> Self {
        let key = build_secret_key(&config);
        Self::with_dependencies(config, db_pool, key, ai_provider, channels)
    }

    pub fn config(&self) -> &Config {
        &self.inner.config
    }
    pub fn db_pool(&self) -> &PgPool {
        &self.inner.db_pool
    }
    pub fn secret_key(&self) -> &SecretKey {
        &self.inner.secret_key
    }
    pub fn login_rate_limiter(&self) -> &RateLimiter {
        &self.inner.login_rate_limiter
    }
    pub fn register_rate_limiter(&self) -> &RateLimiter {
        &self.inner.register_rate_limiter
    }
    pub fn analyze_rate_limiter(&self) -> &UserRateLimiter {
        &self.inner.analyze_rate_limiter
    }
    pub fn channel_test_rate_limiter(&self) -> &UserRateLimiter {
        &self.inner.channel_test_rate_limiter
    }
    pub fn channel_mutation_rate_limiter(&self) -> &UserRateLimiter {
        &self.inner.channel_mutation_rate_limiter
    }
    pub fn ai_provider(&self) -> Arc<dyn AiProvider> {
        Arc::clone(&self.inner.ai_provider)
    }
    pub fn channels(&self) -> Arc<ChannelRegistry> {
        Arc::clone(&self.inner.channels)
    }
}

/// Builds a [`SecretKey`] from configuration.
///
/// Production callers (`Config::load`) have already verified the key
/// is present, so a panic here means the developer bypassed `load()`.
/// Tests may pass any base64 value; if it is absent they fall back to
/// a deterministic all-zeros key that is safe because tests never
/// persist real channel secrets.
fn build_secret_key(config: &Config) -> SecretKey {
    match config.notification_secret_key.as_deref() {
        Some(encoded) => SecretKey::from_base64(encoded)
            .expect("NOTIFICATION_SECRET_KEY must decode to 32 bytes"),
        None => {
            // Test-only fallback. See `Config::load` — the production
            // path refuses to produce a Config without a key.
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode([0u8; 32]);
            SecretKey::from_base64(&encoded).expect("zero key is 32 bytes")
        }
    }
}

fn build_channel_registry(config: &Config) -> ChannelRegistry {
    let client =
        crate::notifications::http::build_client(config.notification_request_timeout_seconds);
    let telegram: Box<dyn crate::notifications::NotificationChannel> =
        Box::new(crate::notifications::telegram::TelegramChannel::new(
            client.clone(),
            config.telegram_bot_token.clone(),
            config.telegram_chat_id.clone(),
        ));
    let discord: Box<dyn crate::notifications::NotificationChannel> =
        Box::new(crate::notifications::discord::DiscordChannel::new(
            client.clone(),
            config.discord_webhook_url.clone(),
        ));
    let webhook: Box<dyn crate::notifications::NotificationChannel> =
        Box::new(crate::notifications::webhook::WebhookChannel::new(
            config.webhook_url.clone(),
            std::time::Duration::from_secs(config.notification_request_timeout_seconds),
            config.webhook_allow_loopback,
        ));
    ChannelRegistry::new(telegram, discord, webhook)
}
