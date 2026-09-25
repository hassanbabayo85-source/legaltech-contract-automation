//! Server-side validation of AI output.
//!
//! Every value that will be written to PostgreSQL passes through here.
//! The AI is treated as untrusted input: malformed or semantically
//! invalid output is rejected as a whole, not partially saved.

use chrono::NaiveDate;

use crate::ai::schema::{AiAnalysis, AiObligation, AiRisk};

/// Analysis output that has passed every validation rule.
#[derive(Debug, Clone)]
pub struct ValidatedAnalysis {
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub risks: Vec<ValidatedRisk>,
    pub obligations: Vec<ValidatedObligation>,
}

#[derive(Debug, Clone)]
pub struct ValidatedRisk {
    pub title: String,
    pub description: String,
    pub risk_level: String,
    pub risk_score: i32,
    pub evidence: String,
}

#[derive(Debug, Clone)]
pub struct ValidatedObligation {
    pub title: String,
    pub description: String,
    pub due_date: Option<NaiveDate>,
    pub responsible_party: Option<String>,
    pub status: String,
    pub risk_level: Option<String>,
}

/// Maximum lengths enforced on AI-produced strings before storing.
/// These mirror the CHECK constraints of the PART 02 schema where they
/// exist; where they do not, we still impose a bound so a runaway model
/// cannot insert unbounded text.
const MAX_TITLE_LEN: usize = 500;
const MAX_TEXT_LEN: usize = 20_000;
const MAX_EVIDENCE_LEN: usize = 20_000;

const VALID_RISK_LEVELS: &[&str] = &["low", "medium", "high", "critical"];
const VALID_OBLIGATION_STATUSES: &[&str] = &["pending", "completed", "overdue", "cancelled"];

/// Validates an [`AiAnalysis`]. On any failure returns a short, safe
/// message describing the rule that was violated — never the offending
/// value (which could contain contract text).
pub fn validate(analysis: AiAnalysis) -> Result<ValidatedAnalysis, String> {
    let start_date =
        parse_optional_date(analysis.contract_dates.start_date.as_deref(), "start_date")?;
    let end_date = parse_optional_date(analysis.contract_dates.end_date.as_deref(), "end_date")?;

    if let (Some(s), Some(e)) = (start_date, end_date) {
        if e < s {
            return Err("end_date is earlier than start_date".to_string());
        }
    }

    let mut risks = Vec::with_capacity(analysis.risks.len());
    for (idx, risk) in analysis.risks.into_iter().enumerate() {
        risks.push(validate_risk(risk, idx)?);
    }

    let mut obligations = Vec::with_capacity(analysis.obligations.len());
    for (idx, obligation) in analysis.obligations.into_iter().enumerate() {
        obligations.push(validate_obligation(obligation, idx)?);
    }

    Ok(ValidatedAnalysis {
        start_date,
        end_date,
        risks,
        obligations,
    })
}

fn parse_optional_date(value: Option<&str>, field: &str) -> Result<Option<NaiveDate>, String> {
    match value {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
            .map(Some)
            .map_err(|_| format!("{field} is not a valid ISO date")),
    }
}

fn validate_risk(risk: AiRisk, idx: usize) -> Result<ValidatedRisk, String> {
    let title = require_bounded("risks[].title", &risk.title, MAX_TITLE_LEN)?;
    let description = require_bounded("risks[].description", &risk.description, MAX_TEXT_LEN)?;
    let evidence = require_bounded("risks[].evidence", &risk.evidence, MAX_EVIDENCE_LEN)?;

    if !VALID_RISK_LEVELS.contains(&risk.risk_level.as_str()) {
        return Err(format!("risks[{idx}].risk_level is not a valid level"));
    }
    if !(0..=100).contains(&risk.risk_score) {
        return Err(format!("risks[{idx}].risk_score must be between 0 and 100"));
    }

    Ok(ValidatedRisk {
        title,
        description,
        risk_level: risk.risk_level,
        risk_score: risk.risk_score,
        evidence,
    })
}

fn validate_obligation(
    obligation: AiObligation,
    idx: usize,
) -> Result<ValidatedObligation, String> {
    let title = require_bounded("obligations[].title", &obligation.title, MAX_TITLE_LEN)?;
    let description = require_bounded(
        "obligations[].description",
        &obligation.description,
        MAX_TEXT_LEN,
    )?;

    let due_date = parse_optional_date(obligation.due_date.as_deref(), "obligations[].due_date")?;

    if !VALID_OBLIGATION_STATUSES.contains(&obligation.status.as_str()) {
        return Err(format!("obligations[{idx}].status is not a valid status"));
    }

    if let Some(level) = obligation.risk_level.as_deref() {
        if !VALID_RISK_LEVELS.contains(&level) {
            return Err(format!(
                "obligations[{idx}].risk_level is not a valid level"
            ));
        }
    }

    let responsible_party = obligation
        .responsible_party
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    Ok(ValidatedObligation {
        title,
        description,
        due_date,
        responsible_party,
        status: obligation.status,
        risk_level: obligation.risk_level,
    })
}

fn require_bounded(field: &str, value: &str, max: usize) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if trimmed.len() > max {
        return Err(format!("{field} exceeds {max} bytes"));
    }
    Ok(trimmed.to_string())
}

/// Normalises text for fuzzy comparison: lowercase, collapse runs of
/// whitespace, strip common punctuation noise. Used only to compare AI
/// evidence against the source contract — never for display.
pub fn normalize_for_match(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_space = true;
    for ch in input.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
        } else if ch == '\u{201C}'
            || ch == '\u{201D}'
            || ch == '\u{2018}'
            || ch == '\u{2019}'
            || ch == '"'
            || ch == '\''
        {
            // Drop curly/straight quotes — models often normalise them.
        } else if ch.is_alphanumeric()
            || ch == '.'
            || ch == ','
            || ch == ':'
            || ch == ';'
            || ch == '('
            || ch == ')'
            || ch == '-'
            || ch == '/'
        {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            last_was_space = false;
        } else {
            // Drop other punctuation.
        }
    }
    out.trim().to_string()
}

/// Returns true if `evidence` appears (case-insensitively, whitespace-
/// normalised) inside `contract_text`.
///
/// Two-pass matching:
///   1. **Exact:** the normalised evidence appears as a substring.
///   2. **Fuzzy:** at least 70% of the evidence's "significant" tokens
///      (length ≥ 4) appear somewhere in the contract. This tolerates
///      minor paraphrasing by the model — `may` → `can`, `terminate` →
///      `terminate`, `agreement` → `agreement` — without accepting
///      wholesale invention.
///
/// Rejects excerpts shorter than 12 normalised characters so that a
/// single common word cannot match.
pub fn evidence_matches_contract(evidence: &str, contract_text: &str) -> bool {
    let e = normalize_for_match(evidence);
    if e.chars().count() < 12 {
        return false;
    }
    let c = normalize_for_match(contract_text);

    // Fast path: exact substring after normalization.
    if c.contains(&e) {
        return true;
    }

    // Fuzzy path: token-overlap on significant tokens. Only tokens
    // with ≥4 characters count, so stopwords ("the", "and", "may") do
    // not inflate the score.
    let e_tokens: Vec<&str> = e.split_whitespace().filter(|t| t.len() >= 4).collect();
    if e_tokens.len() < 3 {
        // Too few significant tokens to make a judgement.
        return false;
    }
    let c_tokens: std::collections::HashSet<&str> = c.split_whitespace().collect();
    let matching = e_tokens.iter().filter(|t| c_tokens.contains(*t)).count();
    // ceil(len * 0.7): multiply first to stay in integer arithmetic.
    let threshold = e_tokens.len().saturating_mul(7).div_ceil(10);
    matching >= threshold
}

/// Rejects a validated analysis whose risks are not backed by the
/// supplied contract text. Returns `Ok(())` only when **every** risk's
/// `evidence` field is found in the contract (as a quote) or is a
/// clearly-marked absence.
///
/// This is the last line of defence against a hallucinating model: the
/// caller (analysis orchestrator) already receives a shape-validated
/// result. Here we verify meaning.
pub fn verify_risk_evidence(
    analysis: &ValidatedAnalysis,
    contract_text: &str,
) -> Result<(), String> {
    for (idx, risk) in analysis.risks.iter().enumerate() {
        if !evidence_matches_contract(&risk.evidence, contract_text) {
            return Err(format!(
                "risk[{idx}] evidence is not present in the source contract"
            ));
        }
    }
    Ok(())
}

/// Returns the fraction (0..=100) of quote-type risks whose evidence was
/// found in the contract. 100 when there are no quote risks.
pub fn evidence_verification_score(analysis: &ValidatedAnalysis, contract_text: &str) -> i32 {
    let total = analysis.risks.len();
    if total == 0 {
        return 100;
    }
    let verified = analysis
        .risks
        .iter()
        .filter(|r| evidence_matches_contract(&r.evidence, contract_text))
        .count();
    ((verified * 100) / total) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::schema::{AiContractDates, AiObligation, AiRisk};

    fn valid_risk() -> AiRisk {
        AiRisk {
            title: "Automatic renewal".to_string(),
            description: "Contract renews automatically".to_string(),
            risk_level: "high".to_string(),
            risk_score: 75,
            evidence: "Clause 4.2: This agreement shall renew automatically...".to_string(),
        }
    }

    fn valid_obligation() -> AiObligation {
        AiObligation {
            title: "Payment due".to_string(),
            description: "Pay within 30 days".to_string(),
            due_date: Some("2026-12-31".to_string()),
            responsible_party: Some("Client".to_string()),
            status: "pending".to_string(),
            risk_level: Some("low".to_string()),
        }
    }

    #[test]
    fn accepts_minimal_valid_analysis() {
        let a = AiAnalysis {
            contract_dates: AiContractDates::default(),
            risks: vec![],
            obligations: vec![],
        };
        let v = validate(a).unwrap();
        assert!(v.start_date.is_none());
        assert!(v.end_date.is_none());
    }

    #[test]
    fn accepts_full_valid_analysis() {
        let a = AiAnalysis {
            contract_dates: AiContractDates {
                start_date: Some("2026-01-01".to_string()),
                end_date: Some("2026-12-31".to_string()),
            },
            risks: vec![valid_risk()],
            obligations: vec![valid_obligation()],
        };
        let v = validate(a).unwrap();
        assert_eq!(v.risks.len(), 1);
        assert_eq!(v.obligations.len(), 1);
    }

    #[test]
    fn rejects_reversed_dates() {
        let a = AiAnalysis {
            contract_dates: AiContractDates {
                start_date: Some("2026-12-31".to_string()),
                end_date: Some("2026-01-01".to_string()),
            },
            risks: vec![],
            obligations: vec![],
        };
        assert!(validate(a).is_err());
    }

    #[test]
    fn rejects_invalid_date_format() {
        let a = AiAnalysis {
            contract_dates: AiContractDates {
                start_date: Some("01/01/2026".to_string()),
                end_date: None,
            },
            risks: vec![],
            obligations: vec![],
        };
        assert!(validate(a).is_err());
    }

    #[test]
    fn null_dates_are_fine() {
        let a = AiAnalysis {
            contract_dates: AiContractDates {
                start_date: None,
                end_date: None,
            },
            risks: vec![],
            obligations: vec![],
        };
        assert!(validate(a).is_ok());
    }

    #[test]
    fn rejects_empty_risk_title() {
        let mut r = valid_risk();
        r.title = "   ".to_string();
        let a = AiAnalysis {
            contract_dates: AiContractDates::default(),
            risks: vec![r],
            obligations: vec![],
        };
        assert!(validate(a).is_err());
    }

    #[test]
    fn rejects_invalid_risk_level() {
        let mut r = valid_risk();
        r.risk_level = "catastrophic".to_string();
        let a = AiAnalysis {
            contract_dates: AiContractDates::default(),
            risks: vec![r],
            obligations: vec![],
        };
        assert!(validate(a).is_err());
    }

    #[test]
    fn rejects_risk_score_above_100() {
        let mut r = valid_risk();
        r.risk_score = 101;
        let a = AiAnalysis {
            contract_dates: AiContractDates::default(),
            risks: vec![r],
            obligations: vec![],
        };
        assert!(validate(a).is_err());
    }

    #[test]
    fn rejects_risk_score_below_0() {
        let mut r = valid_risk();
        r.risk_score = -1;
        let a = AiAnalysis {
            contract_dates: AiContractDates::default(),
            risks: vec![r],
            obligations: vec![],
        };
        assert!(validate(a).is_err());
    }

    #[test]
    fn rejects_empty_evidence() {
        let mut r = valid_risk();
        r.evidence = "".to_string();
        let a = AiAnalysis {
            contract_dates: AiContractDates::default(),
            risks: vec![r],
            obligations: vec![],
        };
        assert!(validate(a).is_err());
    }

    #[test]
    fn rejects_invalid_obligation_status() {
        let mut o = valid_obligation();
        o.status = "snoozed".to_string();
        let a = AiAnalysis {
            contract_dates: AiContractDates::default(),
            risks: vec![],
            obligations: vec![o],
        };
        assert!(validate(a).is_err());
    }

    #[test]
    fn rejects_invalid_obligation_risk_level() {
        let mut o = valid_obligation();
        o.risk_level = Some("unknown".to_string());
        let a = AiAnalysis {
            contract_dates: AiContractDates::default(),
            risks: vec![],
            obligations: vec![o],
        };
        assert!(validate(a).is_err());
    }

    #[test]
    fn empty_responsible_party_becomes_none() {
        let mut o = valid_obligation();
        o.responsible_party = Some("   ".to_string());
        let a = AiAnalysis {
            contract_dates: AiContractDates::default(),
            risks: vec![],
            obligations: vec![o],
        };
        let v = validate(a).unwrap();
        assert!(v.obligations[0].responsible_party.is_none());
    }

    #[test]
    fn null_due_date_is_fine() {
        let mut o = valid_obligation();
        o.due_date = None;
        let a = AiAnalysis {
            contract_dates: AiContractDates::default(),
            risks: vec![],
            obligations: vec![o],
        };
        assert!(validate(a).is_ok());
    }
}
