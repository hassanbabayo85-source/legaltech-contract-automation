//! `notification_channels` row (safe metadata only).
//!
//! Deliberately does **not** include `encrypted_config`. That column
//! is loaded only by the specific store functions that need it, and
//! only ever crosses the encryption boundary inside
//! `notifications::store`.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A row from `notification_channels`, excluding the encrypted
/// configuration blob.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NotificationChannel {
    pub id: Uuid,
    pub user_id: Uuid,
    pub channel_type: String,
    pub name: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_tested_at: Option<DateTime<Utc>>,
    pub last_test_error: Option<String>,
}
