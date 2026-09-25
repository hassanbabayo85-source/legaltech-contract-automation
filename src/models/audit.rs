//! `audit_logs` row.

use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

/// A row from the `audit_logs` table.
///
/// `metadata` is free-form JSONB. Callers must never place secrets
/// (passwords, tokens, API keys, cookies) or raw contract text in it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuditLog {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<Uuid>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}
