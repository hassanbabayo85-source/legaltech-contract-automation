//! `contracts` rows.

use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

/// A full row from the `contracts` table.
///
/// `raw_text` is sensitive: never log it, never include it in audit
/// metadata, never echo it back in a generic error message.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Contract {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub raw_text: String,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub risk_level: Option<String>,
    pub risk_score: Option<i32>,
    pub risk_summary: Option<String>,
    pub content_version: i32,
    pub analysis_status: String,
    pub analyzed_at: Option<DateTime<Utc>>,
    pub analyzed_content_version: Option<i32>,
    pub analysis_provider: Option<String>,
    pub analysis_model: Option<String>,
    pub analysis_prompt_version: Option<i32>,
    pub analysis_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A row from `contracts` **without** `raw_text` or analysis internals,
/// used by the list endpoint.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ContractSummaryRow {
    pub id: Uuid,
    pub title: String,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub risk_level: Option<String>,
    pub risk_score: Option<i32>,
    pub risk_summary: Option<String>,
    pub content_version: i32,
    pub analysis_status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
