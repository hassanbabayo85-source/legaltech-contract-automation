//! Typed models mirroring the backend's JSON DTOs.
//!
//! These are the *wire* types — never the backend's internal database
//! models. Secrets are deliberately absent: the backend does not send
//! them, and no code here should expect them.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// -------------------------------------------------------------------------
// Error envelope
// -------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

// -------------------------------------------------------------------------
// Auth
// -------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub full_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub full_name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthResponse {
    pub user: UserResponse,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

// -------------------------------------------------------------------------
// Contracts
// -------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ContractResponse {
    pub id: Uuid,
    pub title: String,
    #[serde(default)]
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ContractSummary {
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

#[derive(Debug, Clone, Deserialize)]
pub struct PaginatedContracts {
    pub items: Vec<ContractSummary>,
    pub page: u32,
    pub limit: u32,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateContractRequest {
    pub title: String,
    pub raw_text: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateContractRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_text: Option<String>,
}

// -------------------------------------------------------------------------
// AI analysis
// -------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RiskItem {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub risk_level: String,
    pub risk_score: i32,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ObligationItem {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub due_date: Option<NaiveDate>,
    pub responsible_party: Option<String>,
    pub status: String,
    pub risk_level: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AnalysisResponse {
    pub contract_id: Uuid,
    pub analysis_status: String,
    pub content_version: i32,
    pub analyzed_content_version: Option<i32>,
    pub analyzed_at: Option<DateTime<Utc>>,
    pub analysis_provider: Option<String>,
    pub analysis_model: Option<String>,
    pub analysis_prompt_version: Option<i32>,
    pub analysis_error: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub risk_level: Option<String>,
    pub risk_score: Option<i32>,
    pub risk_summary: Option<String>,
    pub risks: Vec<RiskItem>,
    pub obligations: Vec<ObligationItem>,
}

// -------------------------------------------------------------------------
// Reminders
// -------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ReminderResponse {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub obligation_id: Option<Uuid>,
    pub reminder_date: DateTime<Utc>,
    pub reminder_type: String,
    pub channel_type: String,
    pub status: String,
    pub reminder_source: String,
    pub source_content_version: Option<i32>,
    pub sent_at: Option<DateTime<Utc>>,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PaginatedReminders {
    pub items: Vec<ReminderResponse>,
    pub page: u32,
    pub limit: u32,
    pub total: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GenerationResponse {
    pub contract_id: Uuid,
    pub created: usize,
    pub preserved_custom: usize,
    pub skipped_duplicates: usize,
    pub deleted_pending_system: u64,
    pub content_version: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateCustomReminderRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub obligation_id: Option<Uuid>,
    pub reminder_date: DateTime<Utc>,
    pub channel_type: String,
}

// -------------------------------------------------------------------------
// Notification channels
// -------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ChannelResponse {
    pub id: Uuid,
    pub channel_type: String,
    pub name: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_tested_at: Option<DateTime<Utc>>,
    pub last_test_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PaginatedChannels {
    pub items: Vec<ChannelResponse>,
    pub page: u32,
    pub limit: u32,
    pub total: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TestResultResponse {
    pub ok: bool,
    pub error: Option<String>,
}

// -------------------------------------------------------------------------
// Dashboard
// -------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct DashboardSummary {
    pub contracts: DashboardContractCounts,
    pub obligations: DashboardObligationCounts,
    pub reminders: DashboardReminderCounts,
    pub channels: DashboardChannelCounts,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DashboardContractCounts {
    pub total: i64,
    pub analyzed: i64,
    pub pending_analysis: i64,
    pub high_or_critical: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DashboardObligationCounts {
    pub overdue: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DashboardReminderCounts {
    pub pending: i64,
    pub sent: i64,
    pub failed: i64,
    pub upcoming_30d: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DashboardChannelCounts {
    pub enabled: i64,
}
