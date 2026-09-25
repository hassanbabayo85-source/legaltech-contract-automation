//! Typed access to the `contract_risks` table.

use crate::models::ContractRisk;
use uuid::Uuid;

pub async fn insert_risk<'e, E>(
    executor: E,
    contract_id: Uuid,
    title: &str,
    description: &str,
    risk_level: &str,
    risk_score: i32,
    evidence: &str,
) -> Result<ContractRisk, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query_as::<_, ContractRisk>(
        "INSERT INTO contract_risks \
             (contract_id, title, description, risk_level, risk_score, evidence) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id, contract_id, title, description, risk_level, \
                   risk_score, evidence, created_at",
    )
    .bind(contract_id)
    .bind(title)
    .bind(description)
    .bind(risk_level)
    .bind(risk_score)
    .bind(evidence)
    .fetch_one(executor)
    .await
}

pub async fn list_risks_for_contract<'e, E>(
    executor: E,
    contract_id: Uuid,
) -> Result<Vec<ContractRisk>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query_as::<_, ContractRisk>(
        "SELECT id, contract_id, title, description, risk_level, \
                risk_score, evidence, created_at \
         FROM contract_risks WHERE contract_id = $1 \
         ORDER BY risk_score DESC, created_at ASC",
    )
    .bind(contract_id)
    .fetch_all(executor)
    .await
}

pub async fn delete_risks_for_contract<'e, E>(
    executor: E,
    contract_id: Uuid,
) -> Result<u64, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query("DELETE FROM contract_risks WHERE contract_id = $1")
        .bind(contract_id)
        .execute(executor)
        .await
        .map(|r| r.rows_affected())
}
