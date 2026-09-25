//! Typed access to the `contract_obligations` table.

use crate::models::ContractObligation;
use chrono::NaiveDate;
use uuid::Uuid;

const OBLIGATION_COLUMNS: &str = "id, contract_id, title, description, due_date, \
    responsible_party, status, risk_level, created_at, updated_at";

pub async fn insert_obligation<'e, E>(
    executor: E,
    contract_id: Uuid,
    title: &str,
    description: &str,
) -> Result<ContractObligation, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let sql = format!(
        "INSERT INTO contract_obligations (contract_id, title, description) \
         VALUES ($1, $2, $3) RETURNING {OBLIGATION_COLUMNS}"
    );
    sqlx::query_as::<_, ContractObligation>(&sql)
        .bind(contract_id)
        .bind(title)
        .bind(description)
        .fetch_one(executor)
        .await
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_obligation_full<'e, E>(
    executor: E,
    contract_id: Uuid,
    title: &str,
    description: &str,
    due_date: Option<NaiveDate>,
    responsible_party: Option<&str>,
    status: &str,
    risk_level: Option<&str>,
) -> Result<ContractObligation, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let sql = format!(
        "INSERT INTO contract_obligations \
             (contract_id, title, description, due_date, responsible_party, status, risk_level) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {OBLIGATION_COLUMNS}"
    );
    sqlx::query_as::<_, ContractObligation>(&sql)
        .bind(contract_id)
        .bind(title)
        .bind(description)
        .bind(due_date)
        .bind(responsible_party)
        .bind(status)
        .bind(risk_level)
        .fetch_one(executor)
        .await
}

pub async fn list_obligations_for_contract<'e, E>(
    executor: E,
    contract_id: Uuid,
) -> Result<Vec<ContractObligation>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let sql = format!(
        "SELECT {OBLIGATION_COLUMNS} FROM contract_obligations \
         WHERE contract_id = $1 ORDER BY due_date ASC NULLS LAST, created_at ASC"
    );
    sqlx::query_as::<_, ContractObligation>(&sql)
        .bind(contract_id)
        .fetch_all(executor)
        .await
}

pub async fn delete_obligations_for_contract<'e, E>(
    executor: E,
    contract_id: Uuid,
) -> Result<u64, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query("DELETE FROM contract_obligations WHERE contract_id = $1")
        .bind(contract_id)
        .execute(executor)
        .await
        .map(|r| r.rows_affected())
}
