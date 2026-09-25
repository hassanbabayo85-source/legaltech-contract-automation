//! Contract business logic: validation, orchestration, audit logging.
//!
//! Handlers in `src/api/contracts.rs` are HTTP concerns only. Every
//! ownership-sensitive operation goes through this module, which in
//! turn calls `db::contracts` — where the ownership filter is baked
//! into the SQL itself.

use serde_json::json;
use uuid::Uuid;

use crate::db;
use crate::errors::AppError;
use crate::models::{Contract, ContractSummaryRow};
use crate::state::AppState;

/// Maximum `title` length in bytes (matches the CHECK constraint added
/// in migration 0001).
pub const MAX_TITLE_BYTES: usize = 500;

/// Maximum `raw_text` length in bytes. 1 MiB ≈ 500,000 words, well
/// beyond any realistic contract. Enforced in the application layer in
/// addition to the HTTP body-size limit configured on the router.
pub const MAX_RAW_TEXT_BYTES: usize = 1024 * 1024;

/// Default page size for `GET /api/contracts`.
pub const DEFAULT_PAGE_LIMIT: u32 = 20;

/// Maximum page size for `GET /api/contracts`. Requests above this are
/// rejected with 400 rather than silently clamped, so a client cannot
/// accidentally believe it received a full page when it did not.
pub const MAX_PAGE_LIMIT: u32 = 100;

/// Sortable columns for `GET /api/contracts`.
///
/// The enum is the whitelist: any client-supplied value that does not
/// map to one of these variants is rejected. The `as_sql` values are
/// the only strings that ever reach the SQL string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    CreatedAt,
    UpdatedAt,
    EndDate,
    Title,
}

impl SortField {
    pub fn as_sql(self) -> &'static str {
        match self {
            Self::CreatedAt => "created_at",
            Self::UpdatedAt => "updated_at",
            Self::EndDate => "end_date",
            Self::Title => "title",
        }
    }
}

/// Sort direction for `GET /api/contracts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    pub fn as_sql(self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }
}

/// Validates a `title`. Trims the surrounding whitespace. Rejects empty
/// titles, oversized titles, and titles containing control characters
/// (newlines, tabs, NUL, ...) — a contract title is a single line of
/// human-readable text, not free-form prose.
pub fn validate_title(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("title must not be empty".to_string()));
    }
    if trimmed.len() > MAX_TITLE_BYTES {
        return Err(AppError::Validation(format!(
            "title must be at most {MAX_TITLE_BYTES} bytes"
        )));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AppError::Validation(
            "title must not contain control characters".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

/// Validates `raw_text`. Does **not** trim — leading/trailing
/// whitespace, indentation, and blank lines are meaningful in legal
/// text and must be preserved. Only checks that the text contains at
/// least one non-whitespace character and does not exceed the size
/// limit.
pub fn validate_raw_text(raw: &str) -> Result<(), AppError> {
    if raw.trim().is_empty() {
        return Err(AppError::Validation(
            "raw_text must not be empty".to_string(),
        ));
    }
    if raw.len() > MAX_RAW_TEXT_BYTES {
        return Err(AppError::Validation(format!(
            "raw_text must be at most {MAX_RAW_TEXT_BYTES} bytes"
        )));
    }
    Ok(())
}

/// Creates a contract and its audit entry inside a single transaction.
pub async fn create_contract(
    state: &AppState,
    user_id: Uuid,
    title: String,
    raw_text: String,
) -> Result<Contract, AppError> {
    let mut tx = state.db_pool().begin().await.map_err(AppError::Database)?;

    let contract = db::contracts::insert_contract(&mut *tx, user_id, &title, &raw_text)
        .await
        .map_err(AppError::Database)?;

    db::audit::insert_audit(
        &mut *tx,
        Some(user_id),
        "contract_created",
        "contract",
        Some(contract.id),
        json!({}),
    )
    .await
    .map_err(AppError::Database)?;

    tx.commit().await.map_err(AppError::Database)?;

    tracing::info!(
        user_id = %user_id,
        contract_id = %contract.id,
        "contract_created"
    );

    Ok(contract)
}

/// Lists contracts owned by `user_id`.
///
/// Returns the page of summaries plus the total count for pagination
/// metadata. The two reads are not in a transaction — a client paging
/// concurrently with a create may see a `total` that is off by one,
/// which is acceptable for this kind of listing.
pub async fn list_contracts(
    state: &AppState,
    user_id: Uuid,
    page: u32,
    limit: u32,
    sort_field: SortField,
    sort_direction: SortDirection,
) -> Result<(Vec<ContractSummaryRow>, i64), AppError> {
    let limit_i64 = i64::from(limit);
    let offset_i64 = i64::from(page.saturating_sub(1)) * limit_i64;

    let rows = db::contracts::list_contracts_for_user(
        state.db_pool(),
        user_id,
        limit_i64,
        offset_i64,
        sort_field.as_sql(),
        sort_direction.as_sql(),
    )
    .await
    .map_err(AppError::Database)?;

    let total = db::contracts::count_contracts_for_user(state.db_pool(), user_id)
        .await
        .map_err(AppError::Database)?;

    Ok((rows, total))
}

/// Fetches a single contract owned by `user_id`.
///
/// Returns [`AppError::NotFound`] whether the contract does not exist
/// or belongs to another user — the two cases are indistinguishable to
/// the client.
pub async fn get_contract(
    state: &AppState,
    user_id: Uuid,
    contract_id: Uuid,
) -> Result<Contract, AppError> {
    db::contracts::find_contract_for_user(state.db_pool(), contract_id, user_id)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("contract".to_string()))
}

/// Updates `title` and/or `raw_text` on a contract owned by `user_id`.
///
/// When `raw_text` changes, the contract's `content_version` is bumped
/// and `analysis_status` is reset to `'not_analyzed'` (all inside the
/// `UPDATE` statement in `db::contracts::update_contract`).
///
/// The audit entry records which fields were present in the request
/// (not their values), so an auditor can see what was touched without
/// leaking sensitive content.
pub async fn update_contract(
    state: &AppState,
    user_id: Uuid,
    contract_id: Uuid,
    title: Option<String>,
    raw_text: Option<String>,
) -> Result<Contract, AppError> {
    if title.is_none() && raw_text.is_none() {
        return Err(AppError::Validation(
            "at least one of `title` or `raw_text` is required".to_string(),
        ));
    }

    let mut changed_fields: Vec<&'static str> = Vec::new();
    if title.is_some() {
        changed_fields.push("title");
    }
    if raw_text.is_some() {
        changed_fields.push("raw_text");
    }

    let mut tx = state.db_pool().begin().await.map_err(AppError::Database)?;

    let updated = db::contracts::update_contract(
        &mut *tx,
        contract_id,
        user_id,
        title.as_deref(),
        raw_text.as_deref(),
    )
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound("contract".to_string()))?;

    db::audit::insert_audit(
        &mut *tx,
        Some(user_id),
        "contract_updated",
        "contract",
        Some(updated.id),
        json!({ "changed_fields": changed_fields }),
    )
    .await
    .map_err(AppError::Database)?;

    tx.commit().await.map_err(AppError::Database)?;

    tracing::info!(
        user_id = %user_id,
        contract_id = %updated.id,
        content_version = updated.content_version,
        "contract_updated"
    );

    Ok(updated)
}

/// Deletes a contract owned by `user_id`.
///
/// Dependent rows are removed by the `ON DELETE CASCADE` foreign keys
/// from PART 02. Returns [`AppError::NotFound`] if no row was deleted
/// (which covers both "does not exist" and "belongs to another user").
pub async fn delete_contract(
    state: &AppState,
    user_id: Uuid,
    contract_id: Uuid,
) -> Result<(), AppError> {
    let mut tx = state.db_pool().begin().await.map_err(AppError::Database)?;

    let deleted = db::contracts::delete_contract(&mut *tx, contract_id, user_id)
        .await
        .map_err(AppError::Database)?;

    if !deleted {
        return Err(AppError::NotFound("contract".to_string()));
    }

    db::audit::insert_audit(
        &mut *tx,
        Some(user_id),
        "contract_deleted",
        "contract",
        Some(contract_id),
        json!({}),
    )
    .await
    .map_err(AppError::Database)?;

    tx.commit().await.map_err(AppError::Database)?;

    tracing::info!(
        user_id = %user_id,
        contract_id = %contract_id,
        "contract_deleted"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_trims_and_accepts_normal_input() {
        assert_eq!(
            validate_title("  Employment Agreement  ").unwrap(),
            "Employment Agreement"
        );
    }

    #[test]
    fn title_rejects_empty() {
        assert!(validate_title("").is_err());
        assert!(validate_title("   ").is_err());
    }

    #[test]
    fn title_rejects_oversized() {
        let long = "a".repeat(MAX_TITLE_BYTES + 1);
        assert!(validate_title(&long).is_err());
    }

    #[test]
    fn title_rejects_control_characters() {
        assert!(validate_title("bad\ntitle").is_err());
        assert!(validate_title("bad\0title").is_err());
    }

    #[test]
    fn title_accepts_unicode() {
        assert!(validate_title("Yarjejeniyar Aiki").is_ok());
        assert!(validate_title("契約書").is_ok());
    }

    #[test]
    fn raw_text_rejects_empty_and_whitespace_only() {
        assert!(validate_raw_text("").is_err());
        assert!(validate_raw_text("   \n  ").is_err());
    }

    #[test]
    fn raw_text_preserves_internal_whitespace() {
        let input = "  line one\n\n  line two  ";
        assert!(validate_raw_text(input).is_ok());
    }

    #[test]
    fn raw_text_rejects_oversized() {
        let long = "a".repeat(MAX_RAW_TEXT_BYTES + 1);
        assert!(validate_raw_text(&long).is_err());
    }

    #[test]
    fn sort_field_maps_to_whitelisted_sql() {
        assert_eq!(SortField::CreatedAt.as_sql(), "created_at");
        assert_eq!(SortField::UpdatedAt.as_sql(), "updated_at");
        assert_eq!(SortField::EndDate.as_sql(), "end_date");
        assert_eq!(SortField::Title.as_sql(), "title");
    }

    #[test]
    fn sort_direction_maps_to_sql_keyword() {
        assert_eq!(SortDirection::Asc.as_sql(), "ASC");
        assert_eq!(SortDirection::Desc.as_sql(), "DESC");
    }
}
