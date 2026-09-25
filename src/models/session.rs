//! `sessions` row.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A row from the `sessions` table.
///
/// `token_hash` is a credential equivalent: never log it, never return
/// it in any API response, never include it in an audit entry.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Session {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub last_used_at: DateTime<Utc>,
}
