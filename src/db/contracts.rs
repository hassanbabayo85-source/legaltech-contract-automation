//! Typed access to the `contracts` table.
//!
//! Every function includes the ownership filter `user_id = $N` in its
//! SQL. Handlers never fetch by ID and check ownership separately.

use crate::models::{Contract, ContractSummaryRow};
use chrono::NaiveDate;
use uuid::Uuid;

const CONTRACT_COLUMNS: &str = "id, user_id, title, raw_text, start_date, end_date, \
    risk_level, risk_score, risk_summary, content_version, analysis_status, \
    analyzed_at, analyzed_content_version, analysis_provider, analysis_model, \
    analysis_prompt_version, analysis_error, created_at, updated_at";

pub async fn insert_contract<'e, E>(
    executor: E,
    user_id: Uuid,
    title: &str,
    raw_text: &str,
) -> Result<Contract, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let sql = format!(
        "INSERT INTO contracts (user_id, title, raw_text) VALUES ($1, $2, $3) RETURNING {CONTRACT_COLUMNS}"
    );
    sqlx::query_as::<_, Contract>(&sql)
        .bind(user_id)
        .bind(title)
        .bind(raw_text)
        .fetch_one(executor)
        .await
}

pub async fn find_contract_for_user<'e, E>(
    executor: E,
    id: Uuid,
    user_id: Uuid,
) -> Result<Option<Contract>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let sql = format!("SELECT {CONTRACT_COLUMNS} FROM contracts WHERE id = $1 AND user_id = $2");
    sqlx::query_as::<_, Contract>(&sql)
        .bind(id)
        .bind(user_id)
        .fetch_optional(executor)
        .await
}

pub async fn update_contract<'e, E>(
    executor: E,
    id: Uuid,
    user_id: Uuid,
    title: Option<&str>,
    raw_text: Option<&str>,
) -> Result<Option<Contract>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let sql = format!(
        "UPDATE contracts SET \
             title = COALESCE($3, title), \
             raw_text = COALESCE($4, raw_text), \
             content_version = CASE \
                 WHEN $4 IS NOT NULL AND $4 IS DISTINCT FROM raw_text \
                     THEN content_version + 1 \
                 ELSE content_version \
             END, \
             analysis_status = CASE \
                 WHEN $4 IS NOT NULL AND $4 IS DISTINCT FROM raw_text \
                     THEN 'not_analyzed' \
                 ELSE analysis_status \
             END, \
             updated_at = NOW() \
         WHERE id = $1 AND user_id = $2 \
         RETURNING {CONTRACT_COLUMNS}"
    );
    sqlx::query_as::<_, Contract>(&sql)
        .bind(id)
        .bind(user_id)
        .bind(title)
        .bind(raw_text)
        .fetch_optional(executor)
        .await
}

pub async fn delete_contract<'e, E>(
    executor: E,
    id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query("DELETE FROM contracts WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(executor)
        .await
        .map(|r| r.rows_affected() > 0)
}

pub async fn list_contracts_for_user<'e, E>(
    executor: E,
    user_id: Uuid,
    limit: i64,
    offset: i64,
    order_by: &'static str,
    order_direction: &'static str,
) -> Result<Vec<ContractSummaryRow>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let sql = format!(
        "SELECT id, title, start_date, end_date, risk_level, risk_score, \
                risk_summary, content_version, analysis_status, \
                created_at, updated_at \
         FROM contracts \
         WHERE user_id = $1 \
         ORDER BY {order_by} {order_direction} \
         LIMIT $2 OFFSET $3"
    );
    sqlx::query_as::<_, ContractSummaryRow>(&sql)
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(executor)
        .await
}

pub async fn count_contracts_for_user<'e, E>(executor: E, user_id: Uuid) -> Result<i64, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM contracts WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(executor)
        .await
}

// -------------------------------------------------------------------------
// PART 05 — analysis workflow helpers
// -------------------------------------------------------------------------

/// Atomically claims the contract for analysis.
///
/// Succeeds (returns `true`) only if the contract still exists, belongs
/// to `user_id`, is at `content_version`, and is not already `pending`.
/// This is what prevents two concurrent analyze requests from both
/// starting an AI call.
pub async fn claim_analysis<'e, E>(
    executor: E,
    id: Uuid,
    user_id: Uuid,
    content_version: i32,
) -> Result<bool, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query(
        "UPDATE contracts \
         SET analysis_status = 'pending', analysis_error = NULL, updated_at = NOW() \
         WHERE id = $1 AND user_id = $2 AND content_version = $3 \
           AND analysis_status <> 'pending'",
    )
    .bind(id)
    .bind(user_id)
    .bind(content_version)
    .execute(executor)
    .await
    .map(|r| r.rows_affected() > 0)
}

/// Resets `pending` analyses that have been stuck for longer than
/// `stale_seconds`. Called periodically by the worker's recovery sweep.
///
/// A `pending` row with an `updated_at` older than the threshold means
/// the process that claimed it either crashed or was killed before it
/// could mark the row as `completed` or `failed`. Resetting to
/// `not_analyzed` lets the user retry from the UI.
///
/// Returns the number of rows recovered.
pub async fn recover_stale_pending_analyses(
    pool: &sqlx::PgPool,
    stale_seconds: u64,
) -> Result<u64, sqlx::Error> {
    sqlx::query(
        "UPDATE contracts \
         SET analysis_status = 'not_analyzed', \
             analysis_error = 'analysis_recovered_stale', \
             updated_at = NOW() \
         WHERE analysis_status = 'pending' \
           AND updated_at < NOW() - make_interval(secs => $1)",
    )
    .bind(stale_seconds as f64)
    .execute(pool)
    .await
    .map(|r| r.rows_affected())
}

/// Marks the analysis as failed for the given content version.
///
/// No-op if the contract has since changed to a different content
/// version (the caller's analysis is stale and should not touch the
/// newer row).
pub async fn mark_analysis_failed<'e, E>(
    executor: E,
    id: Uuid,
    content_version: i32,
    safe_error: &str,
) -> Result<(), sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query(
        "UPDATE contracts \
         SET analysis_status = 'failed', analysis_error = $3, updated_at = NOW() \
         WHERE id = $1 AND content_version = $2",
    )
    .bind(id)
    .bind(content_version)
    .bind(safe_error)
    .execute(executor)
    .await
    .map(|_| ())
}

/// Writes the analysis result. Returns `None` if the contract's
/// content_version no longer matches `content_version` — the caller must
/// then treat the analysis as stale and roll back.
#[allow(clippy::too_many_arguments)]
pub async fn complete_analysis<'e, E>(
    executor: E,
    id: Uuid,
    content_version: i32,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
    risk_level: Option<&str>,
    risk_score: Option<i32>,
    risk_summary: Option<&str>,
    provider: &str,
    model: &str,
    prompt_version: i32,
) -> Result<Option<Contract>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let sql = format!(
        "UPDATE contracts SET \
             start_date = $3, end_date = $4, \
             risk_level = $5, risk_score = $6, risk_summary = $7, \
             analysis_status = 'completed', \
             analyzed_at = NOW(), \
             analyzed_content_version = $2, \
             analysis_provider = $8, \
             analysis_model = $9, \
             analysis_prompt_version = $10, \
             analysis_error = NULL, \
             updated_at = NOW() \
         WHERE id = $1 AND content_version = $2 \
         RETURNING {CONTRACT_COLUMNS}"
    );
    sqlx::query_as::<_, Contract>(&sql)
        .bind(id)
        .bind(content_version)
        .bind(start_date)
        .bind(end_date)
        .bind(risk_level)
        .bind(risk_score)
        .bind(risk_summary)
        .bind(provider)
        .bind(model)
        .bind(prompt_version)
        .fetch_optional(executor)
        .await
}
