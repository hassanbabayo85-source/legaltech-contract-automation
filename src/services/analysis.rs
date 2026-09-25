//! Contract analysis orchestration.
//!
//! Ties together: ownership check, AI provider call with bounded
//! retries, server-side validation, atomic persistence, and stale-result
//! protection. Handlers call into this module and get either a
//! completed [`Contract`] or a typed [`AppError`].

use std::time::Duration;

use uuid::Uuid;

use crate::ai::prompt::PROMPT_VERSION;
use crate::ai::validation::{self, ValidatedAnalysis};
use crate::ai::{AiAnalysis, AiError, AiProvider};
use crate::db;
use crate::errors::AppError;
use crate::models::Contract;
use crate::state::AppState;

/// Result of a successful analysis run.
pub struct AnalysisOutcome {
    pub contract: Contract,
    pub analysis: ValidatedAnalysis,
}

/// Runs the full analysis workflow for one contract.
pub async fn analyze_contract(
    state: &AppState,
    user_id: Uuid,
    contract_id: Uuid,
) -> Result<AnalysisOutcome, AppError> {
    // 1. Load the contract with ownership enforcement.
    let contract = db::contracts::find_contract_for_user(state.db_pool(), contract_id, user_id)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("contract".to_string()))?;

    // 2. Size check — safe rejection before we spend provider budget.
    let max_chars = state.config().ai_max_input_chars;
    if contract.raw_text.len() > max_chars {
        return Err(AppError::Validation(format!(
            "contract text is too large to analyze (limit: {max_chars} bytes)"
        )));
    }

    // 3. Atomic claim: transition not-pending -> pending for this exact
    //    content version. Concurrent callers observe rows_affected = 0
    //    and receive AnalysisInProgress.
    let claimed = db::contracts::claim_analysis(
        state.db_pool(),
        contract_id,
        user_id,
        contract.content_version,
    )
    .await
    .map_err(AppError::Database)?;

    if !claimed {
        return Err(AppError::AnalysisInProgress);
    }

    tracing::info!(
        user_id = %user_id,
        contract_id = %contract_id,
        content_version = contract.content_version,
        "analysis_started"
    );

    // 4. Call the provider with bounded retries.
    let provider = state.ai_provider();
    let ai_result = call_with_retries(
        provider.as_ref(),
        &contract.raw_text,
        state.config().ai_max_retries,
    )
    .await;

    let raw_analysis = match ai_result {
        Ok(a) => a,
        Err(err) => {
            let safe = err.safe_category();
            let _ = db::contracts::mark_analysis_failed(
                state.db_pool(),
                contract_id,
                contract.content_version,
                &safe,
            )
            .await;

            tracing::warn!(
                user_id = %user_id,
                contract_id = %contract_id,
                content_version = contract.content_version,
                provider = provider.name(),
                failure = %safe,
                "analysis_failed"
            );

            return Err(map_ai_error(err));
        }
    };

    // 5. Validate server-side. Any failure aborts the whole result.
    let validated = match validation::validate(raw_analysis) {
        Ok(v) => v,
        Err(reason) => {
            let _ = db::contracts::mark_analysis_failed(
                state.db_pool(),
                contract_id,
                contract.content_version,
                "invalid_ai_output",
            )
            .await;

            tracing::warn!(
                user_id = %user_id,
                contract_id = %contract_id,
                content_version = contract.content_version,
                reason = %reason,
                "analysis_failed_validation"
            );

            return Err(AppError::ExternalService(
                "the analysis provider returned an invalid result".to_string(),
            ));
        }
    };

    // 5b. Evidence verification. We compare every quoted piece of
    // evidence against the source contract to catch a model that
    // hallucinated a clause. Failure here is treated as a validation
    // failure, not a transient error — re-running the same model on the
    // same text tends to reproduce the same hallucination.
    if let Err(reason) = validation::verify_risk_evidence(&validated, &contract.raw_text) {
        let _ = db::contracts::mark_analysis_failed(
            state.db_pool(),
            contract_id,
            contract.content_version,
            "evidence_not_found_in_contract",
        )
        .await;

        tracing::warn!(
            user_id = %user_id,
            contract_id = %contract_id,
            content_version = contract.content_version,
            reason = %reason,
            "analysis_failed_evidence_verification"
        );

        return Err(AppError::ExternalService(
            "the analysis provider returned evidence that is not in the contract".to_string(),
        ));
    }

    // Compute and log a verification score (0..=100). Persisting it
    // requires a schema column that is not present in this build.
    let verification_score = validation::evidence_verification_score(&validated, &contract.raw_text);

    tracing::info!(
        user_id = %user_id,
        contract_id = %contract_id,
        content_version = contract.content_version,
        verification_score = verification_score,
        "analysis_evidence_verified"
    );

    // 6. Persist atomically.
    let updated = persist_analysis(
        state,
        user_id,
        contract.id,
        contract.content_version,
        &validated,
        provider.as_ref(),
    )
    .await?;

    tracing::info!(
        user_id = %user_id,
        contract_id = %contract_id,
        content_version = contract.content_version,
        provider = provider.name(),
        model = provider.model(),
        risk_count = validated.risks.len(),
        obligation_count = validated.obligations.len(),
        "analysis_completed"
    );

    Ok(AnalysisOutcome {
        contract: updated,
        analysis: validated,
    })
}

async fn persist_analysis(
    state: &AppState,
    user_id: Uuid,
    contract_id: Uuid,
    content_version: i32,
    validated: &ValidatedAnalysis,
    provider: &dyn AiProvider,
) -> Result<Contract, AppError> {
    let mut tx = state.db_pool().begin().await.map_err(AppError::Database)?;

    // Replace any prior risks/obligations for this contract. Since
    // analysis is per-contract (not per-version), this is the correct
    // semantic: the new content version's analysis supersedes the old.
    db::risks::delete_risks_for_contract(&mut *tx, contract_id)
        .await
        .map_err(AppError::Database)?;
    db::obligations::delete_obligations_for_contract(&mut *tx, contract_id)
        .await
        .map_err(AppError::Database)?;

    for risk in &validated.risks {
        db::risks::insert_risk(
            &mut *tx,
            contract_id,
            &risk.title,
            &risk.description,
            &risk.risk_level,
            risk.risk_score,
            &risk.evidence,
        )
        .await
        .map_err(AppError::Database)?;
    }

    for ob in &validated.obligations {
        db::obligations::insert_obligation_full(
            &mut *tx,
            contract_id,
            &ob.title,
            &ob.description,
            ob.due_date,
            ob.responsible_party.as_deref(),
            &ob.status,
            ob.risk_level.as_deref(),
        )
        .await
        .map_err(AppError::Database)?;
    }

    let (top_level, top_score, summary) = summarize(validated);

    let updated = db::contracts::complete_analysis(
        &mut *tx,
        contract_id,
        content_version,
        validated.start_date,
        validated.end_date,
        top_level.as_deref(),
        top_score,
        summary.as_deref(),
        provider.name(),
        provider.model(),
        PROMPT_VERSION,
    )
    .await
    .map_err(AppError::Database)?;

    let updated = match updated {
        Some(c) => c,
        None => {
            // Stale: content_version changed during the AI call. Roll
            // back the risk/obligation replacement — we must not attach
            // this analysis to the newer text.
            tx.rollback().await.map_err(AppError::Database)?;

            tracing::warn!(
                contract_id = %contract_id,
                analyzed_version = content_version,
                "analysis_discarded_stale"
            );

            return Err(AppError::Conflict(
                "the contract was modified during analysis; please retry".to_string(),
            ));
        }
    };

    db::audit::insert_audit(
        &mut *tx,
        Some(user_id),
        "contract_analyzed",
        "contract",
        Some(contract_id),
        serde_json::json!({
            "content_version": content_version,
            "provider": provider.name(),
            "model": provider.model(),
            "prompt_version": PROMPT_VERSION,
            "risk_count": validated.risks.len(),
            "obligation_count": validated.obligations.len(),
        }),
    )
    .await
    .map_err(AppError::Database)?;

    tx.commit().await.map_err(AppError::Database)?;
    Ok(updated)
}

async fn call_with_retries(
    provider: &dyn AiProvider,
    raw_text: &str,
    max_retries: u32,
) -> Result<AiAnalysis, AiError> {
    let mut attempt = 0u32;
    let mut delay = Duration::from_millis(500);
    loop {
        match provider.analyze_contract(raw_text).await {
            Ok(a) => return Ok(a),
            Err(err) if err.is_retryable() && attempt < max_retries => {
                attempt += 1;
                tracing::warn!(
                    attempt = attempt,
                    max_retries = max_retries,
                    category = err.safe_category(),
                    "ai_call_retrying"
                );
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(5));
            }
            Err(err) => return Err(err),
        }
    }
}

fn map_ai_error(err: AiError) -> AppError {
    match err {
        AiError::NotConfigured => AppError::ExternalService(
            "contract analysis is not configured on this server".to_string(),
        ),
        AiError::Timeout => {
            AppError::ExternalService("the analysis provider did not respond in time".to_string())
        }
        AiError::RateLimited => {
            AppError::ExternalService("the analysis provider is rate limiting requests".to_string())
        }
        AiError::Transient => AppError::ExternalService(
            "the analysis provider is temporarily unavailable".to_string(),
        ),
        AiError::Permanent { .. } => {
            AppError::ExternalService("the analysis provider rejected the request".to_string())
        }
        AiError::MalformedResponse => AppError::ExternalService(
            "the analysis provider returned an invalid result".to_string(),
        ),
        AiError::Other => {
            AppError::ExternalService("the analysis provider encountered an error".to_string())
        }
    }
}

fn summarize(analysis: &ValidatedAnalysis) -> (Option<String>, Option<i32>, Option<String>) {
    if analysis.risks.is_empty() {
        // No material risks identified. Explicitly record "low / 0" so
        // the UI shows a clear result instead of dashes. The prompt
        // (PROMPT_VERSION >= 2) treats an empty risks array as valid.
        return (
            None,
            None,
            Some("No AI-identified material risks were verified in the supplied text. This does not certify the contract as safe or legally compliant — review manually.".to_string()),
        );
    }

    let highest_score = analysis
        .risks
        .iter()
        .map(|r| r.risk_score)
        .max()
        .unwrap_or(0);
    let highest_level = analysis
        .risks
        .iter()
        .max_by_key(|r| r.risk_score)
        .map(|r| r.risk_level.clone());

    let summary = format!(
        "{} risk{} identified; highest score {}",
        analysis.risks.len(),
        if analysis.risks.len() == 1 { "" } else { "s" },
        highest_score
    );

    (highest_level, Some(highest_score), Some(summary))
}
