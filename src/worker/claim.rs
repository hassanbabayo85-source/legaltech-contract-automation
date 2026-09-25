//! Atomic reminder claiming.
//!
//! The claim is done entirely inside PostgreSQL:
//!
//! 1. `SELECT id FROM reminders ... FOR UPDATE SKIP LOCKED LIMIT n`
//!    locks up to `n` due rows.
//! 2. `UPDATE reminders SET status = 'processing', ... WHERE id = ANY($ids)`
//!    flips exactly those rows.
//! 3. `COMMIT`.
//!
//! Because steps 1 and 2 run in the same transaction, a second worker
//! running concurrently sees either the locked rows (skipped via
//! `SKIP LOCKED`) or the already-transitioned rows (filtered out by the
//! `status = 'pending'` predicate). Two workers cannot claim the same
//! reminder.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// A reminder that this worker has successfully claimed.
#[derive(Debug, Clone)]
pub struct ClaimedReminder {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub obligation_id: Option<Uuid>,
    pub reminder_date: DateTime<Utc>,
    pub reminder_type: String,
    pub channel_type: String,
    pub attempts: i32,
}

/// Claims up to `batch_size` due reminders for `worker_id`.
///
/// A reminder is "due" when:
///
/// * `status = 'pending'`
/// * `reminder_date <= NOW()`
/// * `next_attempt_at IS NULL OR next_attempt_at <= NOW()`
///
/// Rows are locked with `FOR UPDATE SKIP LOCKED` in deterministic order
/// (`reminder_date ASC, id ASC`) so older reminders drain first.
///
/// Returns an empty vector when there is no work. Callers should sleep
/// for `WORKER_POLL_INTERVAL_SECONDS` before calling again.
pub async fn claim_due(
    pool: &PgPool,
    worker_id: &str,
    batch_size: i32,
) -> Result<Vec<ClaimedReminder>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let claimed: Vec<ClaimedReminder> = sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>, DateTime<Utc>, String, String, i32)>(
        "SELECT id, contract_id, obligation_id, reminder_date, reminder_type, channel_type, attempts \
         FROM reminders \
         WHERE status = 'pending' \
           AND reminder_date <= NOW() \
           AND (next_attempt_at IS NULL OR next_attempt_at <= NOW()) \
         ORDER BY reminder_date ASC, id ASC \
         FOR UPDATE SKIP LOCKED \
         LIMIT $1",
    )
    .bind(batch_size)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|(id, contract_id, obligation_id, reminder_date, reminder_type, channel_type, attempts)| {
        ClaimedReminder {
            id,
            contract_id,
            obligation_id,
            reminder_date,
            reminder_type,
            channel_type,
            attempts,
        }
    })
    .collect();

    if claimed.is_empty() {
        tx.commit().await?;
        return Ok(Vec::new());
    }

    let ids: Vec<Uuid> = claimed.iter().map(|c| c.id).collect();

    sqlx::query(
        "UPDATE reminders \
         SET status = 'processing', \
             claimed_at = NOW(), \
             claimed_by = $2, \
             last_attempt_at = NOW(), \
             updated_at = NOW() \
         WHERE id = ANY($1)",
    )
    .bind(&ids)
    .bind(worker_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(claimed)
}

#[cfg(test)]
mod tests {
    // Integration tests for claim_due live in tests/worker.rs, where a
    // real PostgreSQL is available. Unit tests here would only
    // re-assert SQL semantics that the DB engine already guarantees.
}
