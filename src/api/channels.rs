//! `/api/notification-channels` — CRUD + test endpoint.
//!
//! Every endpoint requires authentication. Ownership is enforced
//! inside SQL. Secrets are never returned by any DTO.

use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthenticatedUser;
use crate::errors::{AppError, AppResult};
use crate::models::NotificationChannel;
use crate::notifications::channel::{Notification, NotificationChannel as _};
use crate::notifications::config::ChannelConfig;
use crate::notifications::store::{self, StoreError};
use crate::state::AppState;

/// Body limit for channel endpoints. Configs are small (a few hundred
/// bytes); 32 KiB is far more than enough.
const CHANNEL_BODY_LIMIT_BYTES: usize = 32 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/:id", get(get_one).patch(update).delete(delete_one))
        .route("/:id/test", post(test_channel))
        .layer(DefaultBodyLimit::max(CHANNEL_BODY_LIMIT_BYTES))
}

// -------------------------------------------------------------------------
// DTOs
// -------------------------------------------------------------------------

// NOTE: `deny_unknown_fields` is deliberately NOT used here. Serde
// does not support combining it with `#[serde(flatten)]` — the
// combination causes *every* input to be rejected. Extra fields sent
// by the client are instead caught by `ChannelConfig`'s own
// `deny_unknown_fields`, which is applied after flattening.
#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    #[serde(flatten)]
    pub config: ChannelConfig,
}

#[derive(Debug, Deserialize)]
pub struct UpdateChannelRequest {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    #[serde(flatten)]
    pub config: Option<ChannelConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ListQueryRaw {
    pub page: Option<String>,
    pub limit: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChannelResponse {
    pub id: Uuid,
    pub channel_type: String,
    pub name: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_tested_at: Option<DateTime<Utc>>,
    pub last_test_error: Option<String>,
}

impl From<NotificationChannel> for ChannelResponse {
    fn from(c: NotificationChannel) -> Self {
        Self {
            id: c.id,
            channel_type: c.channel_type,
            name: c.name,
            enabled: c.enabled,
            created_at: c.created_at,
            updated_at: c.updated_at,
            last_tested_at: c.last_tested_at,
            last_test_error: c.last_test_error,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedChannels {
    pub items: Vec<ChannelResponse>,
    pub page: u32,
    pub limit: u32,
    pub total: i64,
}

#[derive(Debug, Serialize)]
pub struct TestResultResponse {
    pub ok: bool,
    pub error: Option<String>,
}

// -------------------------------------------------------------------------
// Rejection mapping
// -------------------------------------------------------------------------

fn map_json_rejection(err: JsonRejection) -> AppError {
    match err.status() {
        StatusCode::PAYLOAD_TOO_LARGE => AppError::PayloadTooLarge,
        StatusCode::UNPROCESSABLE_ENTITY => AppError::UnprocessableEntity(err.body_text()),
        _ => AppError::Validation(err.body_text()),
    }
}

fn map_path_rejection(_: PathRejection) -> AppError {
    AppError::Validation("path parameter must be a valid UUID".to_string())
}

fn map_query_rejection(_: QueryRejection) -> AppError {
    AppError::Validation("invalid query string".to_string())
}

fn map_store_error(e: StoreError) -> AppError {
    match e {
        StoreError::Database(db) => AppError::Database(db),
        StoreError::Secret(_) => {
            tracing::error!("secret decryption failed");
            AppError::Internal(anyhow::anyhow!("secret decryption failed"))
        }
        StoreError::ConfigJson(_) => {
            tracing::error!("stored config is not valid JSON");
            AppError::Internal(anyhow::anyhow!("stored config invalid"))
        }
    }
}

fn check_user_rate_limit(
    limiter: &crate::middleware::rate_limit::UserRateLimiter,
    user_id: Uuid,
) -> Result<(), AppError> {
    limiter
        .check(user_id)
        .map_err(|retry| AppError::RateLimited {
            retry_after_seconds: retry.as_secs().max(1),
        })
}

struct ParsedListQuery {
    page: u32,
    limit: u32,
}

fn parse_list_query(raw: ListQueryRaw) -> Result<ParsedListQuery, AppError> {
    let page = match raw.page.as_deref() {
        Some(s) => s
            .parse::<u32>()
            .map_err(|_| AppError::Validation("`page` must be a positive integer".to_string()))?,
        None => 1,
    };
    if page == 0 {
        return Err(AppError::Validation("`page` must be >= 1".to_string()));
    }
    let limit = match raw.limit.as_deref() {
        Some(s) => s
            .parse::<u32>()
            .map_err(|_| AppError::Validation("`limit` must be a positive integer".to_string()))?,
        None => 20,
    };
    if limit == 0 {
        return Err(AppError::Validation("`limit` must be >= 1".to_string()));
    }
    if limit > 100 {
        return Err(AppError::Validation("`limit` must be <= 100".to_string()));
    }
    Ok(ParsedListQuery { page, limit })
}

fn validate_name(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("name must not be empty".to_string()));
    }
    if trimmed.len() > 100 {
        return Err(AppError::Validation(
            "name must be at most 100 bytes".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

// -------------------------------------------------------------------------
// Handlers
// -------------------------------------------------------------------------

pub async fn create(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    payload: Result<Json<CreateChannelRequest>, JsonRejection>,
) -> AppResult<impl IntoResponse> {
    check_user_rate_limit(state.channel_mutation_rate_limiter(), auth.user_id)?;
    let Json(req) = payload.map_err(map_json_rejection)?;

    let name = validate_name(&req.name)?;
    req.config.validate().map_err(AppError::Validation)?;

    let channel = match store::insert_channel(
        state.db_pool(),
        auth.user_id,
        req.config.channel_type(),
        &name,
        &req.config,
        state.secret_key(),
    )
    .await
    {
        Ok(c) => c,
        Err(StoreError::Database(sqlx::Error::Database(db_err)))
            if db_err.code().as_deref() == Some("23505") =>
        {
            // The partial unique index enforces at most one *enabled*
            // channel per (user, type). A duplicate insert is a
            // conflict, not a server error.
            return Err(AppError::Conflict(
                "an enabled channel of this type already exists".to_string(),
            ));
        }
        Err(e) => return Err(map_store_error(e)),
    };

    tracing::info!(
        user_id = %auth.user_id,
        channel_id = %channel.id,
        channel_type = %channel.channel_type,
        "notification_channel_created"
    );

    Ok((StatusCode::CREATED, Json(ChannelResponse::from(channel))))
}

pub async fn list(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    query: Result<Query<ListQueryRaw>, QueryRejection>,
) -> AppResult<impl IntoResponse> {
    let Query(raw) = query.map_err(map_query_rejection)?;
    let parsed = parse_list_query(raw)?;

    let limit = i64::from(parsed.limit);
    let offset = i64::from(parsed.page.saturating_sub(1)) * limit;

    let rows = store::list_channels_for_user(state.db_pool(), auth.user_id, limit, offset)
        .await
        .map_err(map_store_error)?;
    let total = store::count_channels_for_user(state.db_pool(), auth.user_id)
        .await
        .map_err(map_store_error)?;

    Ok((
        StatusCode::OK,
        Json(PaginatedChannels {
            items: rows.into_iter().map(ChannelResponse::from).collect(),
            page: parsed.page,
            limit: parsed.limit,
            total,
        }),
    ))
}

pub async fn get_one(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    path: Result<Path<Uuid>, PathRejection>,
) -> AppResult<impl IntoResponse> {
    let Path(id) = path.map_err(map_path_rejection)?;
    let channel = store::find_channel_for_user(state.db_pool(), id, auth.user_id)
        .await
        .map_err(map_store_error)?
        .ok_or_else(|| AppError::NotFound("channel".to_string()))?;
    Ok((StatusCode::OK, Json(ChannelResponse::from(channel))))
}

pub async fn update(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    path: Result<Path<Uuid>, PathRejection>,
    payload: Result<Json<UpdateChannelRequest>, JsonRejection>,
) -> AppResult<impl IntoResponse> {
    check_user_rate_limit(state.channel_mutation_rate_limiter(), auth.user_id)?;
    let Path(id) = path.map_err(map_path_rejection)?;
    let Json(req) = payload.map_err(map_json_rejection)?;

    let name = match req.name.as_deref() {
        Some(n) => Some(validate_name(n)?),
        None => None,
    };
    if let Some(cfg) = &req.config {
        cfg.validate().map_err(AppError::Validation)?;
    }

    let updated = store::update_channel(
        state.db_pool(),
        id,
        auth.user_id,
        name.as_deref(),
        req.enabled,
        req.config.as_ref(),
        state.secret_key(),
    )
    .await
    .map_err(map_store_error)?
    .ok_or_else(|| AppError::NotFound("channel".to_string()))?;

    tracing::info!(
        user_id = %auth.user_id,
        channel_id = %updated.id,
        "notification_channel_updated"
    );

    Ok((StatusCode::OK, Json(ChannelResponse::from(updated))))
}

pub async fn delete_one(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    path: Result<Path<Uuid>, PathRejection>,
) -> AppResult<impl IntoResponse> {
    check_user_rate_limit(state.channel_mutation_rate_limiter(), auth.user_id)?;
    let Path(id) = path.map_err(map_path_rejection)?;

    let removed = store::delete_channel(state.db_pool(), id, auth.user_id)
        .await
        .map_err(map_store_error)?;
    if !removed {
        return Err(AppError::NotFound("channel".to_string()));
    }

    tracing::info!(
        user_id = %auth.user_id,
        channel_id = %id,
        "notification_channel_deleted"
    );

    Ok(StatusCode::NO_CONTENT)
}

pub async fn test_channel(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    path: Result<Path<Uuid>, PathRejection>,
) -> AppResult<impl IntoResponse> {
    check_user_rate_limit(state.channel_test_rate_limiter(), auth.user_id)?;
    let Path(id) = path.map_err(map_path_rejection)?;

    let (channel, config) = store::find_channel_with_config_for_user(
        state.db_pool(),
        id,
        auth.user_id,
        state.secret_key(),
    )
    .await
    .map_err(map_store_error)?
    .ok_or_else(|| AppError::NotFound("channel".to_string()))?;

    if !channel.enabled {
        return Err(AppError::Conflict(
            "channel is disabled; enable it before testing".to_string(),
        ));
    }

    // Build a synthetic notification. Uses no real reminder, no user
    // data, no contract text — just a clearly labelled test payload.
    let notification = Notification {
        reminder_id: Uuid::new_v4(),
        contract_id: Uuid::new_v4(),
        obligation_id: None,
        title: "LexGuard test notification".to_string(),
        message: "This is a test from your LexGuard notification channel. \
                  No action is required."
            .to_string(),
        due_date: None,
        reminder_type: "custom".to_string(),
        channel_type: config.channel_type(),
    };

    // Build a per-config channel instance (do NOT use the process-wide
    // registry, which is for the worker's env-based fallback).
    let outcome = dispatch_test(&state, &config, &notification).await;

    // Record the result. The test error goes into the row as a short
    // category string (never the raw provider response).
    let safe_err = outcome.as_ref().err().map(|e| e.safe_category());
    let _ = store::record_test_result(state.db_pool(), id, auth.user_id, safe_err).await;

    match outcome {
        Ok(()) => Ok((
            StatusCode::OK,
            Json(TestResultResponse {
                ok: true,
                error: None,
            }),
        )),
        Err(err) => {
            tracing::warn!(
                user_id = %auth.user_id,
                channel_id = %id,
                channel_type = %channel.channel_type,
                category = err.safe_category(),
                "notification_channel_test_failed"
            );
            Ok((
                StatusCode::OK,
                Json(TestResultResponse {
                    ok: false,
                    error: Some(err.safe_category().to_string()),
                }),
            ))
        }
    }
}

/// Builds a one-off channel from the config and sends the notification.
///
/// Uses the same channel implementations as the worker, but a fresh
/// instance because env-based config does not apply to user channels.
async fn dispatch_test(
    state: &AppState,
    config: &ChannelConfig,
    notification: &Notification,
) -> Result<(), crate::notifications::channel::DeliveryError> {
    use crate::notifications::discord::DiscordChannel;
    use crate::notifications::http::build_client;
    use crate::notifications::telegram::TelegramChannel;
    use crate::notifications::webhook::WebhookChannel;

    let timeout = state.config().notification_request_timeout_seconds;
    let timeout_dur = std::time::Duration::from_secs(timeout);

    match config {
        ChannelConfig::Telegram(c) => {
            let client = build_client(timeout);
            let ch =
                TelegramChannel::new(client, Some(c.bot_token.clone()), Some(c.chat_id.clone()));
            ch.send(notification).await
        }
        ChannelConfig::Discord(c) => {
            let client = build_client(timeout);
            let ch = DiscordChannel::new(client, Some(c.webhook_url.clone()));
            ch.send(notification).await
        }
        ChannelConfig::Webhook(c) => {
            let ch = WebhookChannel::new(
                Some(c.url.clone()),
                timeout_dur,
                state.config().webhook_allow_loopback,
            );
            let _ = c.auth.as_ref(); // reserved for future auth types
            ch.send(notification).await
        }
    }
}
