//! Typed access to the `sessions` table.

use crate::models::Session;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Inserts a new session and returns the stored row.
///
/// `token_hash` must already be the SHA-256 hex digest of the raw token.
/// This function never sees the raw token.
pub async fn insert_session<'e, E>(
    executor: E,
    user_id: Uuid,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<Session, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query_as::<_, Session>(
        "INSERT INTO sessions (user_id, token_hash, expires_at) \
         VALUES ($1, $2, $3) \
         RETURNING id, user_id, token_hash, expires_at, created_at, revoked_at, last_used_at",
    )
    .bind(user_id)
    .bind(token_hash)
    .bind(expires_at)
    .fetch_one(executor)
    .await
}

/// Looks up an active (not revoked, not expired) session by token hash.
///
/// Returns `None` if no such session exists, or if the row exists but is
/// revoked or expired. Callers must treat all three cases identically —
/// the API must not reveal *why* a token was rejected.
pub async fn find_active_session_by_token_hash<'e, E>(
    executor: E,
    token_hash: &str,
) -> Result<Option<Session>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query_as::<_, Session>(
        "SELECT id, user_id, token_hash, expires_at, created_at, revoked_at, last_used_at \
         FROM sessions \
         WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > NOW()",
    )
    .bind(token_hash)
    .fetch_optional(executor)
    .await
}

/// Updates `last_used_at` to NOW() for the given session.
pub async fn touch_session<'e, E>(executor: E, session_id: Uuid) -> Result<(), sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query("UPDATE sessions SET last_used_at = NOW() WHERE id = $1")
        .bind(session_id)
        .execute(executor)
        .await
        .map(|_| ())
}

/// Marks the given session as revoked. Idempotent: revoking an
/// already-revoked session is a no-op (the `revoked_at` timestamp of the
/// first revocation is preserved).
pub async fn revoke_session<'e, E>(executor: E, session_id: Uuid) -> Result<(), sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query(
        "UPDATE sessions SET revoked_at = NOW() \
         WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(session_id)
    .execute(executor)
    .await
    .map(|_| ())
}

/// Deletes sessions that are expired, or that were revoked before
/// `cutoff`. Returns the number of rows deleted.
///
/// Intended to be called periodically by a future worker (PART 05+); the
/// function is provided now so the mechanism exists and is testable.
pub async fn delete_stale_sessions<'e, E>(
    executor: E,
    cutoff: DateTime<Utc>,
) -> Result<u64, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query(
        "DELETE FROM sessions \
         WHERE expires_at <= NOW() OR (revoked_at IS NOT NULL AND revoked_at <= $1)",
    )
    .bind(cutoff)
    .execute(executor)
    .await
    .map(|r| r.rows_affected())
}
