//! `<Badge>` — a text + border pill used for risk levels and statuses.
//!
//! # Accessibility note
//!
//! Colour is never the only differentiator. The badge always carries a
//! text label, and risk badges also carry a small glyph. This satisfies
//! the "do not rely only on colour" requirement.

use leptos::prelude::*;

#[component]
pub fn Badge(#[prop(into)] label: String, #[prop(into)] kind: String) -> impl IntoView {
    let class = format!("badge badge-{kind}");
    view! { <span class=class>{label}</span> }
}

/// Convenience: builds a `<Badge>` for a risk level string coming from
/// the backend (`low` / `medium` / `high` / `critical`).
#[component]
pub fn RiskBadge(#[prop(into)] level: String) -> impl IntoView {
    let (glyph, label, kind): (&'static str, String, &'static str) =
        match level.to_ascii_lowercase().as_str() {
            "low" => ("●", "Low".to_string(), "risk-low"),
            "medium" => ("◐", "Medium".to_string(), "risk-medium"),
            "high" => ("▲", "High".to_string(), "risk-high"),
            "critical" => ("■", "Critical".to_string(), "risk-critical"),
            other => ("?", other.to_string(), "status-pending"),
        };
    let class = format!("badge badge-{kind}");
    let aria = format!("Risk level: {label}");
    view! {
        <span class=class aria-label=aria>
            <span class="badge-icon" aria-hidden="true">{glyph}</span>
            {label}
        </span>
    }
}

/// Convenience: builds a `<Badge>` for an analysis or reminder status.
#[component]
pub fn StatusBadge(#[prop(into)] status: String) -> impl IntoView {
    let (label, kind): (String, &'static str) = match status.to_ascii_lowercase().as_str() {
        "not_analyzed" => ("Not analyzed".to_string(), "status-not_analyzed"),
        "pending" => ("Pending".to_string(), "status-pending"),
        "processing" => ("Processing".to_string(), "status-processing"),
        "completed" => ("Completed".to_string(), "status-completed"),
        "sent" => ("Sent".to_string(), "status-sent"),
        "failed" => ("Failed".to_string(), "status-failed"),
        "cancelled" => ("Cancelled".to_string(), "status-cancelled"),
        "overdue" => ("Overdue".to_string(), "status-overdue"),
        other => (other.to_string(), "status-pending"),
    };
    let class = format!("badge badge-{kind}");
    let aria = format!("Status: {label}");
    view! {
        <span class=class aria-label=aria>
            {label}
        </span>
    }
}
