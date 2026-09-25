//! The worker's main loop.
//!
//! Lifecycle:
//!
//! 1. On each tick, run the recovery sweep.
//! 2. Claim up to `WORKER_BATCH_SIZE` due reminders atomically.
//! 3. For each, spawn a delivery task gated by
//!    `WORKER_MAX_CONCURRENCY` semaphore permits.
//! 4. Sleep `WORKER_POLL_INTERVAL_SECONDS` (interruptible by the
//!    cancellation token).
//!
//! # Per-user channels (PART 08)
//!
//! Delivery destinations come from `notification_channels` filtered by
//! the contract owner's `user_id`. The env-based channel configuration
//! in `Config` is a fallback for local development and the pre-PART-08
//! tests; it is no longer the primary source.

use std::sync::Arc;
use std::time::Duration;

use futures::stream::{FuturesUnordered, StreamExt};
use sqlx::PgPool;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::notifications::channel::{DeliveryError, Notification, NotificationChannel};
use crate::notifications::config::ChannelConfig;
use crate::notifications::discord::DiscordChannel;
use crate::notifications::http::build_client;
use crate::notifications::secrets::SecretKey;
use crate::notifications::store;
use crate::notifications::telegram::TelegramChannel;
use crate::notifications::webhook::WebhookChannel;
use crate::reminders::types::ChannelType;
use crate::worker::{claim, outcome, recovery};

/// Builds a [`Notification`] from a claimed reminder row.
///
/// Loads the contract's title and the owner's user_id. Does **not**
/// load `raw_text`.
async fn build_notification(
    pool: &PgPool,
    claimed: &claim::ClaimedReminder,
) -> Result<Option<(Notification, uuid::Uuid)>, sqlx::Error> {
    let channel_type = match ChannelType::parse(&claimed.channel_type) {
        Some(c) => c,
        None => return Ok(None),
    };

    let row: Option<(String, uuid::Uuid)> =
        sqlx::query_as("SELECT title, user_id FROM contracts WHERE id = $1")
            .bind(claimed.contract_id)
            .fetch_optional(pool)
            .await?;

    let Some((contract_title, user_id)) = row else {
        return Ok(None);
    };

    let due_date: Option<chrono::NaiveDate> = if let Some(oid) = claimed.obligation_id {
        sqlx::query_scalar("SELECT due_date FROM contract_obligations WHERE id = $1")
            .bind(oid)
            .fetch_optional(pool)
            .await?
            .flatten()
    } else {
        None
    };

    Ok(Some((
        Notification {
            reminder_id: claimed.id,
            contract_id: claimed.contract_id,
            obligation_id: claimed.obligation_id,
            title: contract_title,
            message: format!(
                "Reminder: {}",
                humanize_reminder_type(&claimed.reminder_type)
            ),
            due_date,
            reminder_type: claimed.reminder_type.clone(),
            channel_type,
        },
        user_id,
    )))
}

fn humanize_reminder_type(s: &str) -> &str {
    match s {
        "7_days_before" => "7 days before the deadline",
        "3_days_before" => "3 days before the deadline",
        "1_day_before" => "1 day before the deadline",
        "on_deadline" => "on the deadline",
        "custom" => "custom reminder",
        other => other,
    }
}

/// Builds a one-off channel from a stored config and sends the
/// notification. Uses the same channel implementations as the
/// env-based fallback.
async fn send_via_config(
    config: &ChannelConfig,
    notification: &Notification,
    timeout_secs: u64,
    allow_loopback: bool,
) -> Result<(), DeliveryError> {
    let timeout = std::time::Duration::from_secs(timeout_secs);
    match config {
        ChannelConfig::Telegram(c) => {
            let client = build_client(timeout_secs);
            let ch =
                TelegramChannel::new(client, Some(c.bot_token.clone()), Some(c.chat_id.clone()));
            ch.send(notification).await
        }
        ChannelConfig::Discord(c) => {
            let client = build_client(timeout_secs);
            let ch = DiscordChannel::new(client, Some(c.webhook_url.clone()));
            ch.send(notification).await
        }
        ChannelConfig::Webhook(c) => {
            let ch = WebhookChannel::new(Some(c.url.clone()), timeout, allow_loopback);
            ch.send(notification).await
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn deliver_one(
    pool: PgPool,
    _channels: Arc<ChannelRegistry>,
    secret_key: Arc<SecretKey>,
    config: Arc<Config>,
    worker_id: Arc<String>,
    max_retries: u32,
    claimed: claim::ClaimedReminder,
) {
    let reminder_id = claimed.id;

    let (notification, user_id) = match build_notification(&pool, &claimed).await {
        Ok(Some(x)) => x,
        Ok(None) => {
            let _ = outcome::schedule_retry_or_fail(
                &pool,
                reminder_id,
                &worker_id,
                &DeliveryError::UnknownChannel,
                0,
            )
            .await;
            return;
        }
        Err(err) => {
            tracing::error!(reminder_id = %reminder_id, error = ?err, "build_notification failed");
            let _ = outcome::schedule_retry_or_fail(
                &pool,
                reminder_id,
                &worker_id,
                &DeliveryError::Transient,
                max_retries,
            )
            .await;
            return;
        }
    };

    let channel_type = notification.channel_type;

    // Look up the user's enabled channel of this type.
    let lookup = store::find_enabled_channel_for_user_by_type(&pool, user_id, channel_type).await;
    let (channel_config, _channel_id) = match lookup {
        Ok(Some((channel, encrypted))) => match secret_key.decrypt(&encrypted) {
            Ok(bytes) => match serde_json::from_slice::<ChannelConfig>(&bytes) {
                Ok(cfg) => (cfg, Some(channel.id)),
                Err(_) => {
                    tracing::error!(
                        reminder_id = %reminder_id,
                        "stored channel config is invalid JSON"
                    );
                    let _ = outcome::schedule_retry_or_fail(
                        &pool,
                        reminder_id,
                        &worker_id,
                        &DeliveryError::NotConfigured,
                        0,
                    )
                    .await;
                    return;
                }
            },
            Err(_) => {
                tracing::error!(
                    reminder_id = %reminder_id,
                    "channel config decryption failed"
                );
                let _ = outcome::schedule_retry_or_fail(
                    &pool,
                    reminder_id,
                    &worker_id,
                    &DeliveryError::NotConfigured,
                    0,
                )
                .await;
                return;
            }
        },
        Ok(None) => {
            tracing::warn!(
                reminder_id = %reminder_id,
                user_id = %user_id,
                channel = %channel_type.as_str(),
                "no enabled channel configured for user"
            );
            let _ = outcome::schedule_retry_or_fail(
                &pool,
                reminder_id,
                &worker_id,
                &DeliveryError::NotConfigured,
                0,
            )
            .await;
            return;
        }
        Err(err) => {
            tracing::error!(
                reminder_id = %reminder_id,
                error = ?err,
                "channel lookup failed"
            );
            let _ = outcome::schedule_retry_or_fail(
                &pool,
                reminder_id,
                &worker_id,
                &DeliveryError::Transient,
                max_retries,
            )
            .await;
            return;
        }
    };

    // Skip if config type disagrees with reminder type (defensive).
    if channel_config.channel_type() != channel_type {
        let _ = outcome::schedule_retry_or_fail(
            &pool,
            reminder_id,
            &worker_id,
            &DeliveryError::UnknownChannel,
            0,
        )
        .await;
        return;
    }

    let start = std::time::Instant::now();
    let timeout_secs = config.notification_request_timeout_seconds;
    let allow_loopback = config.webhook_allow_loopback;
    let result =
        send_via_config(&channel_config, &notification, timeout_secs, allow_loopback).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(()) => match outcome::mark_sent(&pool, reminder_id, &worker_id).await {
            Ok(true) => tracing::info!(
                reminder_id = %reminder_id,
                contract_id = %claimed.contract_id,
                channel = %channel_type.as_str(),
                duration_ms,
                "notification_sent"
            ),
            Ok(false) => tracing::warn!(
                reminder_id = %reminder_id,
                "delivered but state transition lost; recovery will reattempt"
            ),
            Err(err) => tracing::error!(
                reminder_id = %reminder_id,
                error = ?err,
                "failed to mark sent"
            ),
        },
        Err(err) => {
            tracing::warn!(
                reminder_id = %reminder_id,
                channel = %channel_type.as_str(),
                category = err.safe_category(),
                retryable = err.is_retryable(),
                duration_ms,
                "notification_failed"
            );
            let budget = if err.is_retryable() { max_retries } else { 0 };
            let _ =
                outcome::schedule_retry_or_fail(&pool, reminder_id, &worker_id, &err, budget).await;
        }
    }
}

/// Registry mapping [`ChannelType`] to a concrete env-configured
/// channel. Kept for the AI/analyze routes that use it for outbound
/// (none today) and for tests.
#[derive(Debug)]
pub struct ChannelRegistry {
    telegram: Box<dyn NotificationChannel>,
    discord: Box<dyn NotificationChannel>,
    webhook: Box<dyn NotificationChannel>,
}

impl ChannelRegistry {
    pub fn new(
        telegram: Box<dyn NotificationChannel>,
        discord: Box<dyn NotificationChannel>,
        webhook: Box<dyn NotificationChannel>,
    ) -> Self {
        Self {
            telegram,
            discord,
            webhook,
        }
    }

    pub fn channel_for(&self, t: ChannelType) -> Option<&dyn NotificationChannel> {
        match t {
            ChannelType::Telegram => Some(self.telegram.as_ref()),
            ChannelType::Discord => Some(self.discord.as_ref()),
            ChannelType::Webhook => Some(self.webhook.as_ref()),
        }
    }

    pub async fn send(&self, t: ChannelType, n: &Notification) -> Result<(), DeliveryError> {
        let ch = self.channel_for(t).ok_or(DeliveryError::UnknownChannel)?;
        ch.send(n).await
    }
}

impl ChannelRegistry {
    /// A registry whose channels report `NotConfigured` on every
    /// send. Intended only for tests that do not exercise delivery.
    pub fn test_stub() -> Self {
        use reqwest::Client;
        Self::new(
            Box::new(TelegramChannel::new(Client::new(), None, None)),
            Box::new(DiscordChannel::new(Client::new(), None)),
            Box::new(WebhookChannel::new(
                None,
                std::time::Duration::from_secs(5),
                false,
            )),
        )
    }
}

pub fn make_worker_id() -> String {
    let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());
    let pid = std::process::id();
    let suffix: u32 = rand::random();
    format!("{host}:{pid}:{suffix:08x}")
}

#[allow(clippy::too_many_arguments)]
pub fn spawn(
    config: Config,
    pool: PgPool,
    channels: Arc<ChannelRegistry>,
    secret_key: Arc<SecretKey>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    let poll_interval = Duration::from_secs(config.worker_poll_interval_seconds);
    let batch_size = config.worker_batch_size as i32;
    let max_concurrency = config.worker_max_concurrency;
    let claim_timeout = config.worker_claim_timeout_seconds;
    let shutdown_timeout = Duration::from_secs(config.worker_shutdown_timeout_seconds);
    let max_retries = config.notification_max_retries;
    let worker_id = Arc::new(make_worker_id());
    let config = Arc::new(config);

    tokio::spawn(async move {
        tracing::info!(
            worker_id = %worker_id,
            poll_interval_secs = poll_interval.as_secs(),
            batch_size,
            max_concurrency,
            "worker_started"
        );

        let semaphore = Arc::new(Semaphore::new(max_concurrency as usize));
        let mut in_flight: FuturesUnordered<tokio::task::JoinHandle<()>> = FuturesUnordered::new();
        let mut ticks = tokio::time::interval(poll_interval);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!(worker_id = %worker_id, "worker_cancel_received");
                    break;
                }
                _ = ticks.tick() => {
                    match recovery::recover_stale_processing(&pool, claim_timeout).await {
                        Ok(0) => {}
                        Ok(n) => tracing::warn!(recovered = n, "recovered_stale_processing"),
                        Err(err) => tracing::error!(error = ?err, "recovery_sweep_failed"),
                    }

                    // Analysis-side recovery: reset contracts stuck in
                    // 'pending' for more than 15 minutes (likely a crash
                    // mid-analysis). Generous threshold so a genuinely
                    // slow reasoning model is never interrupted.
                    const ANALYSIS_STALE_SECS: u64 = 900;
                    match recovery::recover_stale_analyses(&pool, ANALYSIS_STALE_SECS).await {
                        Ok(0) => {}
                        Ok(n) => tracing::warn!(recovered = n, "recovered_stale_analyses"),
                        Err(err) => tracing::error!(error = ?err, "analysis_recovery_failed"),
                    }

                    let claimed = match claim::claim_due(&pool, &worker_id, batch_size).await {
                        Ok(v) => v,
                        Err(err) => { tracing::error!(error = ?err, "claim_due_failed"); continue; }
                    };

                    if claimed.is_empty() { continue; }

                    tracing::debug!(count = claimed.len(), "claimed_reminders");

                    for c in claimed {
                        let permit = match semaphore.clone().acquire_owned().await {
                            Ok(p) => p,
                            Err(_) => { tracing::info!("semaphore closed; stopping"); break; }
                        };
                        let pool = pool.clone();
                        let channels = Arc::clone(&channels);
                        let secret_key = Arc::clone(&secret_key);
                        let config = Arc::clone(&config);
                        let worker_id = Arc::clone(&worker_id);
                        in_flight.push(tokio::spawn(async move {
                            let _permit = permit;
                            deliver_one(pool, channels, secret_key, config, worker_id, max_retries, c).await;
                        }));
                    }

                    while in_flight.next().now_or_never().flatten().is_some() {}
                }
            }
        }

        let drain = async { while in_flight.next().await.is_some() {} };
        if tokio::time::timeout(shutdown_timeout, drain).await.is_err() {
            tracing::warn!(worker_id = %worker_id, "shutdown_timeout_elapsed; aborting");
        }

        tracing::info!(worker_id = %worker_id, "worker_stopped");
    })
}

#[allow(dead_code)]
fn _documented_types() {
    let _ = JoinSet::<()>::new();
}

use futures::FutureExt as _;
