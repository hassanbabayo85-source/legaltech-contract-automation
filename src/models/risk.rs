//! `contract_risks` row.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A row from the `contract_risks` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ContractRisk {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub title: String,
    pub description: String,
    pub risk_level: String,
    pub risk_score: i32,
    pub evidence: String,
    pub created_at: DateTime<Utc>,
}
