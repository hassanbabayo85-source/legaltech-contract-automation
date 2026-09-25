//! `reminders` row.

use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Reminder {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub obligation_id: Option<Uuid>,
    pub reminder_date: DateTime<Utc>,
    pub reminder_type: String,
    pub channel_type: String,
    pub status: String,
    pub sent_at: Option<DateTime<Utc>>,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub reminder_source: String,
    pub source_content_version: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
