//! Typed access to the `reminders` table.

use crate::models::Reminder;
use chrono::{DateTime, Utc};
use uuid::Uuid;

const REMINDER_COLUMNS: &str = "id, contract_id, obligation_id, reminder_date, \
    reminder_type, channel_type, status, sent_at, attempts, last_error, \
    reminder_source, source_content_version, created_at, updated_at";

#[allow(clippy::too_many_arguments)]
pub async fn insert_reminder<'e, E>(
    executor: E,
    contract_id: Uuid,
    obligation_id: Option<Uuid>,
    reminder_date: DateTime<Utc>,
    reminder_type: &str,
    channel_type: &str,
) -> Result<Reminder, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let sql = format!(
        "INSERT INTO reminders \
             (contract_id, obligation_id, reminder_date, reminder_type, channel_type) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING {REMINDER_COLUMNS}"
    );
    sqlx::query_as::<_, Reminder>(&sql)
        .bind(contract_id)
        .bind(obligation_id)
        .bind(reminder_date)
        .bind(reminder_type)
        .bind(channel_type)
        .fetch_one(executor)
        .await
}

pub async fn delete_pending_system_reminders<'e, E>(
    executor: E,
    contract_id: Uuid,
) -> Result<u64, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query(
        "DELETE FROM reminders \
         WHERE contract_id = $1 AND reminder_source = 'system' AND status = 'pending'",
    )
    .bind(contract_id)
    .execute(executor)
    .await
    .map(|r| r.rows_affected())
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_system_reminder<'e, E>(
    executor: E,
    contract_id: Uuid,
    obligation_id: Option<Uuid>,
    reminder_date: DateTime<Utc>,
    reminder_type: &str,
    channel_type: &str,
    source_content_version: i32,
) -> Result<Option<Reminder>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let sql = format!(
        "INSERT INTO reminders \
             (contract_id, obligation_id, reminder_date, reminder_type, \
              channel_type, reminder_source, source_content_version) \
         VALUES ($1, $2, $3, $4, $5, 'system', $6) \
         ON CONFLICT DO NOTHING \
         RETURNING {REMINDER_COLUMNS}"
    );
    sqlx::query_as::<_, Reminder>(&sql)
        .bind(contract_id)
        .bind(obligation_id)
        .bind(reminder_date)
        .bind(reminder_type)
        .bind(channel_type)
        .bind(source_content_version)
        .fetch_optional(executor)
        .await
}

pub async fn insert_custom_reminder<'e, E>(
    executor: E,
    contract_id: Uuid,
    obligation_id: Option<Uuid>,
    reminder_date: DateTime<Utc>,
    channel_type: &str,
) -> Result<Reminder, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let sql = format!(
        "INSERT INTO reminders \
             (contract_id, obligation_id, reminder_date, reminder_type, \
              channel_type, reminder_source) \
         VALUES ($1, $2, $3, 'custom', $4, 'custom') \
         RETURNING {REMINDER_COLUMNS}"
    );
    sqlx::query_as::<_, Reminder>(&sql)
        .bind(contract_id)
        .bind(obligation_id)
        .bind(reminder_date)
        .bind(channel_type)
        .fetch_one(executor)
        .await
}

pub async fn list_reminders_for_contract<'e, E>(
    executor: E,
    contract_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<Reminder>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let sql = format!(
        "SELECT {REMINDER_COLUMNS} FROM reminders \
         WHERE contract_id = $1 \
         ORDER BY reminder_date ASC, created_at ASC \
         LIMIT $2 OFFSET $3"
    );
    sqlx::query_as::<_, Reminder>(&sql)
        .bind(contract_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(executor)
        .await
}

pub async fn count_reminders_for_contract<'e, E>(
    executor: E,
    contract_id: Uuid,
) -> Result<i64, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM reminders WHERE contract_id = $1")
        .bind(contract_id)
        .fetch_one(executor)
        .await
}

pub async fn find_reminder_for_user<'e, E>(
    executor: E,
    reminder_id: Uuid,
    user_id: Uuid,
) -> Result<Option<Reminder>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    // Note: `cols` already carries the `r.` prefix, so the template
    // must NOT add another one (that would produce `r.r.id`).
    let sql = format!(
        "SELECT {cols} FROM reminders r \
         INNER JOIN contracts c ON c.id = r.contract_id \
         WHERE r.id = $1 AND c.user_id = $2",
        cols = REMINDER_COLUMNS
            .split(", ")
            .map(|c| format!("r.{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    sqlx::query_as::<_, Reminder>(&sql)
        .bind(reminder_id)
        .bind(user_id)
        .fetch_optional(executor)
        .await
}

pub async fn cancel_reminder_for_user<'e, E>(
    executor: E,
    reminder_id: Uuid,
    user_id: Uuid,
) -> Result<Option<Reminder>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    // Use a subquery for the ownership check instead of UPDATE ... FROM.
    // `UPDATE ... FROM ... RETURNING` in PostgreSQL 18 has ambiguous
    // behaviour around the target-table alias in RETURNING; a subquery
    // avoids that entirely and reads more clearly.
    let sql = format!(
        "UPDATE reminders \
         SET status = 'cancelled', updated_at = NOW() \
         WHERE id = $1 \
           AND status = 'pending' \
           AND contract_id IN (SELECT id FROM contracts WHERE user_id = $2) \
         RETURNING {REMINDER_COLUMNS}"
    );
    sqlx::query_as::<_, Reminder>(&sql)
        .bind(reminder_id)
        .bind(user_id)
        .fetch_optional(executor)
        .await
}

pub async fn contract_owned_by<'e, E>(
    executor: E,
    contract_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contracts WHERE id = $1 AND user_id = $2)",
    )
    .bind(contract_id)
    .bind(user_id)
    .fetch_one(executor)
    .await
}
