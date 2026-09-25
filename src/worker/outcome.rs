//! Reminder state updates after a delivery attempt.
//!
//! Every update is **conditional** on the reminder still being in
//! `processing` and still being claimed by this worker. If another
//! process has changed the state (e.g. the user cancelled it, or the
//! recovery sweep reset it), our update is a no-op and we log that.
//! This is what makes the worker safe against a concurrent
//! cancellation or a lease-expiry race.

use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::notifications::channel::DeliveryError;

/// Marks a reminder as successfully sent.
///
/// Only the worker that still holds the claim may do this. Returns
/// `true` if the row was actually updated.
pub async fn mark_sent(
    pool: &PgPool,
    reminder_id: Uuid,
    worker_id: &str,
) -> Result<bool, sqlx::Error> {
    let rows = sqlx::query(
        "UPDATE reminders \
         SET status = 'sent', \
             sent_at = NOW(), \
             last_error = NULL, \
             attempts = attempts + 1, \
             claimed_at = NULL, \
             claimed_by = NULL, \
             updated_at = NOW() \
         WHERE id = $1 \
           AND status = 'processing' \
           AND claimed_by = $2",
    )
    .bind(reminder_id)
    .bind(worker_id)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows > 0)
}

/// Schedules a retry, or gives up if the retry budget is exhausted.
///
/// * `attempts` is incremented.
/// * `last_error` is set to the safe category string.
/// * If `attempts + 1 < max_retries`: `status` stays `pending`,
///   `next_attempt_at` is set according to exponential backoff.
/// * Otherwise: `status = 'failed'`.
///
/// Returns `true` if the reminder will be retried, `false` if it was
/// permanently failed. Both are successes from the caller's point of
/// view — the return value is only informative.
pub async fn schedule_retry_or_fail(
    pool: &PgPool,
    reminder_id: Uuid,
    worker_id: &str,
    err: &DeliveryError,
    max_retries: u32,
) -> Result<bool, sqlx::Error> {
    // Compute backoff. The current attempt count is `attempts + 1`
    // because we're about to increment it.
    let backoff = compute_backoff(err, 0);
    let _ = backoff; // decided inside the SQL below

    let rows_still_owned: Option<(i32,)> = sqlx::query_as(
        "SELECT attempts FROM reminders \
         WHERE id = $1 AND status = 'processing' AND claimed_by = $2",
    )
    .bind(reminder_id)
    .bind(worker_id)
    .fetch_optional(pool)
    .await?;

    let Some((current_attempts,)) = rows_still_owned else {
        return Ok(false); // Lost the claim; leave the row alone.
    };

    let next_attempt = current_attempts + 1;
    // A permanent error is never retried, regardless of the retry
    // budget. Retryable errors are retried until `max_retries` is
    // reached.
    let will_retry = err.is_retryable() && (next_attempt as u32) < max_retries;

    if will_retry {
        let delay = compute_backoff(err, current_attempts as u32);
        sqlx::query(
            "UPDATE reminders \
             SET status = 'pending', \
                 attempts = attempts + 1, \
                 last_error = $3, \
                 next_attempt_at = NOW() + make_interval(secs => $4), \
                 claimed_at = NULL, \
                 claimed_by = NULL, \
                 updated_at = NOW() \
             WHERE id = $1 AND status = 'processing' AND claimed_by = $2",
        )
        .bind(reminder_id)
        .bind(worker_id)
        .bind(err.safe_category())
        .bind(delay.as_seconds_f64())
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            "UPDATE reminders \
             SET status = 'failed', \
                 attempts = attempts + 1, \
                 last_error = $3, \
                 claimed_at = NULL, \
                 claimed_by = NULL, \
                 updated_at = NOW() \
             WHERE id = $1 AND status = 'processing' AND claimed_by = $2",
        )
        .bind(reminder_id)
        .bind(worker_id)
        .bind(err.safe_category())
        .execute(pool)
        .await?;
    }

    Ok(will_retry)
}

/// Exponential backoff with jitter, honouring `Retry-After` where the
/// provider supplied it.
///
/// Formula: `min(5min, 30s * 2^attempt) + jitter(0..30s)`.
pub fn compute_backoff(err: &DeliveryError, attempt: u32) -> Duration {
    if let Some(secs) = err.retry_after_seconds() {
        return Duration::seconds(secs as i64);
    }

    let exp = 30i64.saturating_mul(2i64.saturating_pow(attempt.min(10)));
    let capped = exp.min(300);
    let jitter = (Utc::now().timestamp_subsec_micros() % 30_000_000) as i64 / 1_000_000; // 0..30s
    Duration::seconds(capped + jitter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_takes_priority() {
        let err = DeliveryError::RateLimited {
            retry_after_seconds: Some(120),
        };
        assert_eq!(compute_backoff(&err, 0).num_seconds(), 120);
    }

    #[test]
    fn backoff_grows_exponentially_and_caps() {
        let err = DeliveryError::Transient;
        let b0 = compute_backoff(&err, 0).num_seconds();
        let b1 = compute_backoff(&err, 1).num_seconds();
        let b10 = compute_backoff(&err, 10).num_seconds();
        assert!((30..=60).contains(&b0));
        assert!((60..=90).contains(&b1));
        assert!(b10 <= 330, "capped at 300s + up to 30s jitter: {b10}");
    }
}
