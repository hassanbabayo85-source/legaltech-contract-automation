//! Crash recovery for `processing` reminders.
//!
//! A worker can crash after claiming a reminder but before committing
//! the outcome. Without recovery the row stays in `processing` forever.
//!
//! The sweep resets any `processing` row whose `claimed_at` is older
//! than `WORKER_CLAIM_TIMEOUT_SECONDS` back to `pending`, clearing the
//! claim fields. `attempts` is NOT incremented here — the delivery may
//! not have happened at all, and the outcome path is what counts
//! attempts. (If the delivery did happen, the retry is exactly the
//! at-least-once semantics we document.)

use sqlx::PgPool;

/// Resets stale `processing` reminders to `pending`.
///
/// Returns the number of reminders recovered.
pub async fn recover_stale_processing(
    pool: &PgPool,
    claim_timeout_seconds: u64,
) -> Result<u64, sqlx::Error> {
    let rows = sqlx::query(
        "UPDATE reminders \
         SET status = 'pending', \
             claimed_at = NULL, \
             claimed_by = NULL, \
             updated_at = NOW() \
         WHERE status = 'processing' \
           AND claimed_at IS NOT NULL \
           AND claimed_at < NOW() - make_interval(secs => $1)",
    )
    .bind(claim_timeout_seconds as f64)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows)
}

/// Analysis-side crash recovery.
///
/// A crash during AI analysis can leave `contracts.analysis_status` at
/// `'pending'` forever. We reset any such row whose `updated_at` is
/// older than `stale_seconds` back to `'not_analyzed'`, so the user can
/// retry from the UI.
///
/// The threshold is generous (default 15 minutes) to avoid racing a
/// genuinely slow analysis. Reasoning models can take 30–120 s; 15
/// minutes leaves a wide margin.
pub async fn recover_stale_analyses(
    pool: &sqlx::PgPool,
    stale_seconds: u64,
) -> Result<u64, sqlx::Error> {
    crate::db::contracts::recover_stale_pending_analyses(pool, stale_seconds).await
}

