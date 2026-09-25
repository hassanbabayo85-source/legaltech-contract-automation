//! Reminder generation, custom-reminder creation, cancellation, listing.

use chrono::{NaiveDate, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::db;
use crate::errors::{AppError, AppResult};
use crate::models::Reminder;
use crate::reminders::dates::{date_to_utc_midnight, system_reminders_for};
use crate::reminders::types::ChannelType;
use crate::state::AppState;

pub const DEFAULT_PAGE_LIMIT: u32 = 50;
pub const MAX_PAGE_LIMIT: u32 = 200;

#[derive(Debug)]
pub struct GenerationResult {
    pub contract_id: Uuid,
    pub created: usize,
    pub preserved_custom: usize,
    pub skipped_duplicates: usize,
    pub deleted_pending_system: u64,
    pub content_version: i32,
}

pub async fn regenerate_system_reminders(
    state: &AppState,
    user_id: Uuid,
    contract_id: Uuid,
) -> AppResult<GenerationResult> {
    let contract = db::contracts::find_contract_for_user(state.db_pool(), contract_id, user_id)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("contract".to_string()))?;

    if contract.analysis_status != "completed" {
        return Err(AppError::Conflict(
            "contract analysis must be completed before reminders can be generated".to_string(),
        ));
    }
    if contract.analyzed_content_version != Some(contract.content_version) {
        return Err(AppError::Conflict(
            "contract analysis is stale; re-run analysis before generating reminders".to_string(),
        ));
    }

    let today_utc: NaiveDate = Utc::now().date_naive();

    let obligations = db::obligations::list_obligations_for_contract(state.db_pool(), contract_id)
        .await
        .map_err(AppError::Database)?;

    struct Target {
        obligation_id: Option<Uuid>,
        reminder_date: chrono::DateTime<Utc>,
        reminder_type: &'static str,
    }
    let mut targets: Vec<Target> = Vec::new();

    if let Some(end_date) = contract.end_date {
        for c in system_reminders_for(end_date, today_utc) {
            targets.push(Target {
                obligation_id: None,
                reminder_date: date_to_utc_midnight(c.reminder_date),
                reminder_type: c.reminder_type.as_str(),
            });
        }
    }

    for ob in &obligations {
        if let Some(due_date) = ob.due_date {
            for c in system_reminders_for(due_date, today_utc) {
                targets.push(Target {
                    obligation_id: Some(ob.id),
                    reminder_date: date_to_utc_midnight(c.reminder_date),
                    reminder_type: c.reminder_type.as_str(),
                });
            }
        }
    }

    let mut tx = state.db_pool().begin().await.map_err(AppError::Database)?;

    let deleted = db::reminders::delete_pending_system_reminders(&mut *tx, contract_id)
        .await
        .map_err(AppError::Database)?;

    let mut created = 0usize;
    let mut skipped = 0usize;
    let default_channel = ChannelType::Telegram.as_str();

    for t in &targets {
        let inserted = db::reminders::insert_system_reminder(
            &mut *tx,
            contract_id,
            t.obligation_id,
            t.reminder_date,
            t.reminder_type,
            default_channel,
            contract.content_version,
        )
        .await
        .map_err(AppError::Database)?;

        if inserted.is_some() {
            created += 1;
        } else {
            skipped += 1;
        }
    }

    let preserved_custom: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM reminders \
         WHERE contract_id = $1 AND reminder_source = 'custom'",
    )
    .bind(contract_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    db::audit::insert_audit(
        &mut *tx,
        Some(user_id),
        "contract_reminders_regenerated",
        "contract",
        Some(contract_id),
        json!({
            "created": created,
            "skipped_duplicates": skipped,
            "preserved_custom": preserved_custom,
            "deleted_pending_system": deleted,
            "content_version": contract.content_version,
        }),
    )
    .await
    .map_err(AppError::Database)?;

    tx.commit().await.map_err(AppError::Database)?;

    tracing::info!(
        user_id = %user_id,
        contract_id = %contract_id,
        content_version = contract.content_version,
        created = created,
        skipped = skipped,
        deleted = deleted,
        "reminders_regenerated"
    );

    Ok(GenerationResult {
        contract_id,
        created,
        preserved_custom: preserved_custom as usize,
        skipped_duplicates: skipped,
        deleted_pending_system: deleted,
        content_version: contract.content_version,
    })
}

pub async fn create_custom(
    state: &AppState,
    user_id: Uuid,
    contract_id: Uuid,
    obligation_id: Option<Uuid>,
    reminder_date: chrono::DateTime<Utc>,
    channel_type: ChannelType,
) -> AppResult<Reminder> {
    let owned = db::reminders::contract_owned_by(state.db_pool(), contract_id, user_id)
        .await
        .map_err(AppError::Database)?;
    if !owned {
        return Err(AppError::NotFound("contract".to_string()));
    }

    if let Some(ob_id) = obligation_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM contract_obligations WHERE id = $1 AND contract_id = $2)",
        )
        .bind(ob_id)
        .bind(contract_id)
        .fetch_one(state.db_pool())
        .await
        .map_err(AppError::Database)?;
        if !exists {
            return Err(AppError::Validation(
                "obligation does not belong to the specified contract".to_string(),
            ));
        }
    }

    let mut tx = state.db_pool().begin().await.map_err(AppError::Database)?;

    let reminder = db::reminders::insert_custom_reminder(
        &mut *tx,
        contract_id,
        obligation_id,
        reminder_date,
        channel_type.as_str(),
    )
    .await
    .map_err(AppError::Database)?;

    db::audit::insert_audit(
        &mut *tx,
        Some(user_id),
        "reminder_created",
        "reminder",
        Some(reminder.id),
        json!({
            "contract_id": contract_id,
            "obligation_id": obligation_id,
            "channel_type": channel_type.as_str(),
        }),
    )
    .await
    .map_err(AppError::Database)?;

    tx.commit().await.map_err(AppError::Database)?;

    tracing::info!(
        user_id = %user_id,
        contract_id = %contract_id,
        reminder_id = %reminder.id,
        channel = channel_type.as_str(),
        "reminder_created"
    );

    Ok(reminder)
}

pub async fn list(
    state: &AppState,
    user_id: Uuid,
    contract_id: Uuid,
    page: u32,
    limit: u32,
) -> AppResult<(Vec<Reminder>, i64)> {
    let owned = db::reminders::contract_owned_by(state.db_pool(), contract_id, user_id)
        .await
        .map_err(AppError::Database)?;
    if !owned {
        return Err(AppError::NotFound("contract".to_string()));
    }

    let limit_i64 = i64::from(limit);
    let offset_i64 = i64::from(page.saturating_sub(1)) * limit_i64;

    let items = db::reminders::list_reminders_for_contract(
        state.db_pool(),
        contract_id,
        limit_i64,
        offset_i64,
    )
    .await
    .map_err(AppError::Database)?;
    let total = db::reminders::count_reminders_for_contract(state.db_pool(), contract_id)
        .await
        .map_err(AppError::Database)?;

    Ok((items, total))
}

pub async fn cancel(state: &AppState, user_id: Uuid, reminder_id: Uuid) -> AppResult<Reminder> {
    let existing = db::reminders::find_reminder_for_user(state.db_pool(), reminder_id, user_id)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("reminder".to_string()))?;

    if existing.status != "pending" {
        return Err(AppError::Conflict(format!(
            "reminder is `{}` and cannot be cancelled",
            existing.status
        )));
    }

    let mut tx = state.db_pool().begin().await.map_err(AppError::Database)?;

    let cancelled = db::reminders::cancel_reminder_for_user(&mut *tx, reminder_id, user_id)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::Conflict("reminder state changed; retry".to_string()))?;

    db::audit::insert_audit(
        &mut *tx,
        Some(user_id),
        "reminder_cancelled",
        "reminder",
        Some(cancelled.id),
        json!({ "contract_id": cancelled.contract_id }),
    )
    .await
    .map_err(AppError::Database)?;

    tx.commit().await.map_err(AppError::Database)?;

    tracing::info!(
        user_id = %user_id,
        reminder_id = %cancelled.id,
        "reminder_cancelled"
    );

    Ok(cancelled)
}
