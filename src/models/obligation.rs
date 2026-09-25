//! `contract_obligations` row.

use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

/// A row from the `contract_obligations` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ContractObligation {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub title: String,
    pub description: String,
    pub due_date: Option<NaiveDate>,
    pub responsible_party: Option<String>,
    pub status: String,
    pub risk_level: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
