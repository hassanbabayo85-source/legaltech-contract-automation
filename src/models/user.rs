//! `users` row.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A row from the `users` table.
///
/// The `password_hash` field is a credential: never log it, never include
/// it in an API response, never put it in an audit entry. It is present
/// here because rows must be decoded faithfully; treating it as sensitive
/// is the caller's responsibility.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub full_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
