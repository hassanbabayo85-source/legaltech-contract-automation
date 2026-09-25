//! Aggregate queries for the dashboard summary endpoint.
//!
//! Every query filters by `user_id`. All counts are returned in one
//! round trip to avoid N+1 on the dashboard.

use uuid::Uuid;

/// Aggregate counts for the authenticated user's dashboard.
///
/// Field names match the JSON keys in the response DTO.
#[derive(Debug, Clone, Copy)]
pub struct DashboardCounts {
    pub total_contracts: i64,
    pub analyzed_contracts: i64,
    pub pending_analysis_contracts: i64,
    pub high_or_critical_contracts: i64,
    pub overdue_obligations: i64,
    pub pending_reminders: i64,
    pub sent_reminders: i64,
    pub failed_reminders: i64,
    pub upcoming_reminders_30d: i64,
    pub enabled_channels: i64,
}

/// Loads every count the dashboard needs in a single round trip.
pub async fn load_dashboard_counts<'e, E>(
    executor: E,
    user_id: Uuid,
) -> Result<DashboardCounts, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let row: (
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
    ) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM contracts WHERE user_id = $1), \
            (SELECT COUNT(*) FROM contracts WHERE user_id = $1 AND analysis_status = 'completed'), \
            (SELECT COUNT(*) FROM contracts WHERE user_id = $1 AND analysis_status IN ('not_analyzed', 'pending')), \
            (SELECT COUNT(*) FROM contracts WHERE user_id = $1 AND risk_level IN ('high', 'critical')), \
            (SELECT COUNT(*) FROM contract_obligations o \
                INNER JOIN contracts c ON c.id = o.contract_id \
                WHERE c.user_id = $1 AND o.due_date IS NOT NULL AND o.due_date < CURRENT_DATE AND o.status <> 'completed'), \
            (SELECT COUNT(*) FROM reminders r \
                INNER JOIN contracts c ON c.id = r.contract_id \
                WHERE c.user_id = $1 AND r.status = 'pending'), \
            (SELECT COUNT(*) FROM reminders r \
                INNER JOIN contracts c ON c.id = r.contract_id \
                WHERE c.user_id = $1 AND r.status = 'sent'), \
            (SELECT COUNT(*) FROM reminders r \
                INNER JOIN contracts c ON c.id = r.contract_id \
                WHERE c.user_id = $1 AND r.status = 'failed'), \
            (SELECT COUNT(*) FROM reminders r \
                INNER JOIN contracts c ON c.id = r.contract_id \
                WHERE c.user_id = $1 AND r.status = 'pending' \
                  AND r.reminder_date >= NOW() \
                  AND r.reminder_date <= NOW() + INTERVAL '30 days'), \
            (SELECT COUNT(*) FROM notification_channels WHERE user_id = $1 AND enabled = TRUE)",
    )
    .bind(user_id)
    .fetch_one(executor)
    .await?;

    Ok(DashboardCounts {
        total_contracts: row.0,
        analyzed_contracts: row.1,
        pending_analysis_contracts: row.2,
        high_or_critical_contracts: row.3,
        overdue_obligations: row.4,
        pending_reminders: row.5,
        sent_reminders: row.6,
        failed_reminders: row.7,
        upcoming_reminders_30d: row.8,
        enabled_channels: row.9,
    })
}
