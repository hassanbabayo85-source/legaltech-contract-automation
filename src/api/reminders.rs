//! `/api/contracts/:id/reminders` and `/api/reminders/:id/cancel`.

use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthenticatedUser;
use crate::errors::{AppError, AppResult};
use crate::models::Reminder;
use crate::reminders::service::{self, DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT};
use crate::reminders::types::{ChannelType, ReminderSource, ReminderStatus, ReminderType};
use crate::state::AppState;

pub fn contract_scoped_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create_custom))
        .route("/generate", post(generate))
}

pub fn standalone_router() -> Router<AppState> {
    Router::new().route("/:id/cancel", post(cancel))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCustomReminderRequest {
    #[serde(default)]
    pub obligation_id: Option<Uuid>,
    pub reminder_date: DateTime<Utc>,
    pub channel_type: String,
}

#[derive(Debug, Deserialize)]
pub struct ListQueryRaw {
    pub page: Option<String>,
    pub limit: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReminderResponse {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub obligation_id: Option<Uuid>,
    pub reminder_date: DateTime<Utc>,
    pub reminder_type: String,
    pub channel_type: String,
    pub status: String,
    pub reminder_source: String,
    pub source_content_version: Option<i32>,
    pub sent_at: Option<DateTime<Utc>>,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Reminder> for ReminderResponse {
    fn from(r: Reminder) -> Self {
        Self {
            id: r.id,
            contract_id: r.contract_id,
            obligation_id: r.obligation_id,
            reminder_date: r.reminder_date,
            reminder_type: r.reminder_type,
            channel_type: r.channel_type,
            status: r.status,
            reminder_source: r.reminder_source,
            source_content_version: r.source_content_version,
            sent_at: r.sent_at,
            attempts: r.attempts,
            last_error: r.last_error,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedReminders {
    pub items: Vec<ReminderResponse>,
    pub page: u32,
    pub limit: u32,
    pub total: i64,
}

#[derive(Debug, Serialize)]
pub struct GenerationResponse {
    pub contract_id: Uuid,
    pub created: usize,
    pub preserved_custom: usize,
    pub skipped_duplicates: usize,
    pub deleted_pending_system: u64,
    pub content_version: i32,
}

fn map_json_rejection(err: JsonRejection) -> AppError {
    match err.status() {
        StatusCode::PAYLOAD_TOO_LARGE => AppError::PayloadTooLarge,
        StatusCode::UNPROCESSABLE_ENTITY => AppError::UnprocessableEntity(err.body_text()),
        _ => AppError::Validation(err.body_text()),
    }
}

fn map_path_rejection(_err: PathRejection) -> AppError {
    AppError::Validation("path parameter must be a valid UUID".to_string())
}

fn map_query_rejection(_err: QueryRejection) -> AppError {
    AppError::Validation("invalid query string".to_string())
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
        None => DEFAULT_PAGE_LIMIT,
    };
    if limit == 0 {
        return Err(AppError::Validation("`limit` must be >= 1".to_string()));
    }
    if limit > MAX_PAGE_LIMIT {
        return Err(AppError::Validation(format!(
            "`limit` must be <= {MAX_PAGE_LIMIT}"
        )));
    }
    Ok(ParsedListQuery { page, limit })
}

pub async fn generate(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    path: Result<Path<Uuid>, PathRejection>,
) -> AppResult<impl IntoResponse> {
    let Path(contract_id) = path.map_err(map_path_rejection)?;
    let result = service::regenerate_system_reminders(&state, auth.user_id, contract_id).await?;
    Ok((
        StatusCode::OK,
        Json(GenerationResponse {
            contract_id: result.contract_id,
            created: result.created,
            preserved_custom: result.preserved_custom,
            skipped_duplicates: result.skipped_duplicates,
            deleted_pending_system: result.deleted_pending_system,
            content_version: result.content_version,
        }),
    ))
}

pub async fn create_custom(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    path: Result<Path<Uuid>, PathRejection>,
    payload: Result<Json<CreateCustomReminderRequest>, JsonRejection>,
) -> AppResult<impl IntoResponse> {
    let Path(contract_id) = path.map_err(map_path_rejection)?;
    let Json(req) = payload.map_err(map_json_rejection)?;

    let channel = ChannelType::parse(&req.channel_type).ok_or_else(|| {
        AppError::Validation(format!(
            "`channel_type` must be one of: telegram, discord, webhook (got `{}`)",
            req.channel_type
        ))
    })?;

    let reminder = service::create_custom(
        &state,
        auth.user_id,
        contract_id,
        req.obligation_id,
        req.reminder_date,
        channel,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(ReminderResponse::from(reminder))))
}

pub async fn list(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    path: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<ListQueryRaw>, QueryRejection>,
) -> AppResult<impl IntoResponse> {
    let Path(contract_id) = path.map_err(map_path_rejection)?;
    let Query(raw) = query.map_err(map_query_rejection)?;
    let parsed = parse_list_query(raw)?;

    let (items, total) =
        service::list(&state, auth.user_id, contract_id, parsed.page, parsed.limit).await?;

    Ok((
        StatusCode::OK,
        Json(PaginatedReminders {
            items: items.into_iter().map(ReminderResponse::from).collect(),
            page: parsed.page,
            limit: parsed.limit,
            total,
        }),
    ))
}

pub async fn cancel(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    path: Result<Path<Uuid>, PathRejection>,
) -> AppResult<impl IntoResponse> {
    let Path(reminder_id) = path.map_err(map_path_rejection)?;
    let reminder = service::cancel(&state, auth.user_id, reminder_id).await?;
    Ok((StatusCode::OK, Json(ReminderResponse::from(reminder))))
}

#[allow(dead_code)]
fn _documented_enums() {
    let _ = ReminderType::Custom;
    let _ = ReminderSource::System;
    let _ = ReminderStatus::Pending;
}
