//! `/api/dashboard/*` — aggregate views for the frontend dashboard.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::auth::AuthenticatedUser;
use crate::db;
use crate::errors::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/summary", get(summary))
}

#[derive(Debug, Serialize)]
pub struct DashboardSummary {
    pub contracts: ContractCounts,
    pub obligations: ObligationCounts,
    pub reminders: ReminderCounts,
    pub channels: ChannelCounts,
}

#[derive(Debug, Serialize)]
pub struct ContractCounts {
    pub total: i64,
    pub analyzed: i64,
    pub pending_analysis: i64,
    pub high_or_critical: i64,
}

#[derive(Debug, Serialize)]
pub struct ObligationCounts {
    pub overdue: i64,
}

#[derive(Debug, Serialize)]
pub struct ReminderCounts {
    pub pending: i64,
    pub sent: i64,
    pub failed: i64,
    pub upcoming_30d: i64,
}

#[derive(Debug, Serialize)]
pub struct ChannelCounts {
    pub enabled: i64,
}

pub async fn summary(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> AppResult<impl IntoResponse> {
    let counts = db::dashboard::load_dashboard_counts(state.db_pool(), auth.user_id)
        .await
        .map_err(AppError::Database)?;

    Ok((
        StatusCode::OK,
        Json(DashboardSummary {
            contracts: ContractCounts {
                total: counts.total_contracts,
                analyzed: counts.analyzed_contracts,
                pending_analysis: counts.pending_analysis_contracts,
                high_or_critical: counts.high_or_critical_contracts,
            },
            obligations: ObligationCounts {
                overdue: counts.overdue_obligations,
            },
            reminders: ReminderCounts {
                pending: counts.pending_reminders,
                sent: counts.sent_reminders,
                failed: counts.failed_reminders,
                upcoming_30d: counts.upcoming_reminders_30d,
            },
            channels: ChannelCounts {
                enabled: counts.enabled_channels,
            },
        }),
    ))
}
