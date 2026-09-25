//! The structured shape the AI must produce.
//!
//! These types are `Deserialize` with lenient defaults so a well-formed
//! response is not rejected just because the model added an extra field
//! or omitted one that is legitimately optional. Semantic validation
//! happens in [`crate::ai::validation`]; this layer only checks that the
//! JSON is structurally the right shape.

use serde::{Deserialize, Serialize};

/// Top-level response from the AI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiAnalysis {
    #[serde(default)]
    pub contract_dates: AiContractDates,
    #[serde(default)]
    pub risks: Vec<AiRisk>,
    #[serde(default)]
    pub obligations: Vec<AiObligation>,
}

/// Dates extracted from the contract. Either may be `null` when the
/// contract does not state them explicitly.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiContractDates {
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
}

/// One risk the AI identified.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRisk {
    pub title: String,
    pub description: String,
    pub risk_level: String,
    pub risk_score: i32,
    pub evidence: String,
}

/// One obligation the AI identified.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiObligation {
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub responsible_party: Option<String>,
    #[serde(default = "default_obligation_status")]
    pub status: String,
    #[serde(default)]
    pub risk_level: Option<String>,
}

fn default_obligation_status() -> String {
    "pending".to_string()
}
