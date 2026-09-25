//! Minimal typed access to the `audit_logs` table.

use crate::models::AuditLog;
use serde_json::Value;
use uuid::Uuid;

/// Inserts an audit entry and returns the stored row.
///
/// `metadata` MUST NOT contain secrets (passwords, tokens, API keys,
/// cookies) or raw contract text.
pub async fn insert_audit<'e, E>(
    executor: E,
    user_id: Option<Uuid>,
    action: &str,
    entity_type: &str,
    entity_id: Option<Uuid>,
    metadata: Value,
) -> Result<AuditLog, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query_as::<_, AuditLog>(
        "INSERT INTO audit_logs (user_id, action, entity_type, entity_id, metadata) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING id, user_id, action, entity_type, entity_id, metadata, created_at",
    )
    .bind(user_id)
    .bind(action)
    .bind(entity_type)
    .bind(entity_id)
    .bind(metadata)
    .fetch_one(executor)
    .await
}
