//! Risks and obligations sections for the contract detail page.

use leptos::prelude::*;

use crate::api::models::{AnalysisResponse, ObligationItem, RiskItem};
use crate::components::{RiskBadge, SafeText, StatusBadge};

/// The full risks + obligations view. Handles its own filtering state.
#[component]
pub fn RisksAndObligations(analysis: AnalysisResponse) -> impl IntoView {
    let risks = analysis.risks.clone();
    let obligations = analysis.obligations.clone();
    let filter = RwSignal::new("all".to_string());

    view! {
        <div class="mt-5">
            <RisksSection risks=risks filter=filter />
            <ObligationsSection obligations=obligations />
        </div>
    }
}

// =========================================================================
// Risks
// =========================================================================

#[component]
pub fn RisksSection(risks: Vec<RiskItem>, filter: RwSignal<String>) -> impl IntoView {
    let total = risks.len();

    let critical_count = risks
        .iter()
        .filter(|r| r.risk_level.eq_ignore_ascii_case("critical"))
        .count();
    let high_count = risks
        .iter()
        .filter(|r| r.risk_level.eq_ignore_ascii_case("high"))
        .count();
    let medium_count = risks
        .iter()
        .filter(|r| r.risk_level.eq_ignore_ascii_case("medium"))
        .count();
    let low_count = risks
        .iter()
        .filter(|r| r.risk_level.eq_ignore_ascii_case("low"))
        .count();

    let filtered = Memo::new({
        let risks = risks.clone();
        move |_| {
            let f = filter.get();
            if f == "all" {
                risks.clone()
            } else {
                risks
                    .iter()
                    .filter(|r| r.risk_level.eq_ignore_ascii_case(&f))
                    .cloned()
                    .collect()
            }
        }
    });

    let show_empty_risks = total == 0;

    view! {
        <section aria-labelledby="risks-heading">
            <div class="row row-between mb-2">
                <h3 id="risks-heading">"AI-identified risks"</h3>
                <span class="text-sm muted">{format!("{total} total")}</span>
            </div>

            <p class="text-xs subtle mb-3">
                "Risks are identified by AI. Treat them as starting points for review, not as definitive legal conclusions."
            </p>

            {if show_empty_risks {
                view! {
                    <div class="card">
                        <p class="muted">
                            "No AI-identified risks were found for this analysis."
                        </p>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="row mb-3" role="group" aria-label="Filter risks by level">
                        <FilterChip filter=filter value="all" label="All" count=total />
                        <FilterChip filter=filter value="critical" label="Critical" count=critical_count />
                        <FilterChip filter=filter value="high" label="High" count=high_count />
                        <FilterChip filter=filter value="medium" label="Medium" count=medium_count />
                        <FilterChip filter=filter value="low" label="Low" count=low_count />
                    </div>

                    <div>
                        <For
                            each=move || filtered.get()
                            key=|r| r.id
                            children=|r| view! { <RiskCard risk=r /> }
                        />
                    </div>
                }.into_any()
            }}
        </section>
    }
}

#[component]
fn FilterChip(
    filter: RwSignal<String>,
    value: &'static str,
    label: &'static str,
    count: usize,
) -> impl IntoView {
    let is_active = move || filter.get() == value;
    view! {
        <button
            class="btn btn-ghost btn-sm"
            type="button"
            aria-pressed=is_active
            style=move || if is_active() {
                "background: var(--c-primary); color: var(--c-primary-fg);"
            } else {
                ""
            }
            on:click=move |_| filter.set(value.to_string())
        >
            {format!("{label} ({count})")}
        </button>
    }
}

#[component]
fn RiskCard(risk: RiskItem) -> impl IntoView {
    let level = risk.risk_level.clone();
    let score = risk.risk_score;
    let title = risk.title.clone();
    let description = risk.description.clone();
    let evidence = risk.evidence.clone();
    let id = risk.id;

    view! {
        <article class="card" aria-labelledby=format!("risk-{id}-title")>
            <div class="row row-between mb-2" style="flex-wrap: wrap;">
                <h4 id=format!("risk-{id}-title") style="margin: 0;">
                    <SafeText text=title />
                </h4>
                <div class="row" style="gap: 0.5rem;">
                    <RiskBadge level=level />
                    <span class="text-xs subtle mono" aria-label="Risk score">
                        {format!("{score}/100")}
                    </span>
                </div>
            </div>

            <p class="text-sm" style="margin: 0.5rem 0;">
                <SafeText text=description />
            </p>

            <div
                class="text-xs subtle"
                style="border-top: 1px solid var(--c-border); padding-top: 0.5rem; margin-top: 0.5rem;"
            >
                <div class="stat-label" style="margin-bottom: 0.25rem;">"Evidence"</div>
                <div style="font-style: italic;">
                    <SafeText text=evidence pre_wrap=true />
                </div>
            </div>
        </article>
    }
}

// =========================================================================
// Obligations
// =========================================================================

#[component]
pub fn ObligationsSection(obligations: Vec<ObligationItem>) -> impl IntoView {
    let total = obligations.len();
    let obligations_clone = obligations.clone();

    view! {
        <section class="mt-5" aria-labelledby="obligations-heading">
            <div class="row row-between mb-2">
                <h3 id="obligations-heading">"Obligations"</h3>
                <span class="text-sm muted">{format!("{total} total")}</span>
            </div>

            {if total == 0 {
                view! {
                    <div class="card">
                        <p class="muted">"No obligations were extracted from this contract."</p>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="table-wrap">
                        <table class="table">
                            <thead>
                                <tr>
                                    <th scope="col">"Title"</th>
                                    <th scope="col">"Due"</th>
                                    <th scope="col">"Responsible"</th>
                                    <th scope="col">"Status"</th>
                                    <th scope="col">"Risk"</th>
                                </tr>
                            </thead>
                            <tbody>
                                <For
                                    each=move || obligations_clone.clone()
                                    key=|o| o.id
                                    children=|o| view! { <ObligationRow obligation=o /> }
                                />
                            </tbody>
                        </table>
                    </div>
                }.into_any()
            }}
        </section>
    }
}

#[component]
fn ObligationRow(obligation: ObligationItem) -> impl IntoView {
    let title = obligation.title.clone();
    let description = obligation.description.clone();
    let due = obligation
        .due_date
        .map(|d| d.format("%Y-%m-%d").to_string());
    let responsible = obligation.responsible_party.clone();
    let status = obligation.status.clone();
    let risk = obligation.risk_level.clone();
    let id = obligation.id;

    view! {
        <tr>
            <td>
                <div style="font-weight: 500;">
                    <SafeText text=title />
                </div>
                <div class="text-xs subtle mt-1">
                    <SafeText text=description />
                </div>
                <div class="text-xs subtle mono">{id.to_string()}</div>
            </td>
            <td class="mono">
                {due.unwrap_or_else(|| "—".to_string())}
            </td>
            <td class="text-sm">
                {responsible.unwrap_or_else(|| "—".to_string())}
            </td>
            <td><StatusBadge status=status /></td>
            <td>
                {risk.map(|lvl| view! { <RiskBadge level=lvl /> })}
            </td>
        </tr>
    }
}
