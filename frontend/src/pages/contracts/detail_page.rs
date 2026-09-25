//! `/contracts/:id` — contract detail with tabbed sections.
//!
//! Tabs (client-side only — no route change):
//!   * Summary    — risk score donut, dates, AI metadata, raw text
//!   * Risks      — every AI-identified risk with filtering
//!   * Obligations — every extracted obligation
//!   * Reminders  — scheduled reminders for this contract
//!
//! "Key Terms" from the design mockup is **not** included: the AI
//! schema does not extract key terms, and inventing that tab would
//! misrepresent what the analysis actually produces.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use uuid::Uuid;

use crate::api::models::{AnalysisResponse, ContractResponse};
use crate::api::ApiError;
use crate::auth::context::use_auth;
use crate::components::icons::{IconEdit, IconTrash};
use crate::components::{
    AppShell, ConfirmDialog, ErrorState, PageLoading, RiskBadge, SafeText, Spinner, StatusBadge,
};
use crate::pages::contracts::detail::{
    ContractEditForm, ObligationsSection, RemindersSection, RisksSection,
};
use crate::pages::dashboard::Disclaimer;
use crate::state::use_toasts;

/// State machine for the page.
#[derive(Debug, Clone)]
enum PageState {
    Loading,
    NotFound,
    Loaded {
        contract: Box<ContractResponse>,
        analysis: Box<AnalysisResponse>,
    },
    Error(ApiError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveTab {
    Summary,
    Risks,
    Obligations,
    Reminders,
}

#[component]
pub fn ContractDetailPage() -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();
    let params = use_params_map();

    let id = {
        let raw = params.read().get("id").unwrap_or_default();
        Uuid::parse_str(&raw).ok()
    };

    let state = RwSignal::new(PageState::Loading);
    let reload = RwSignal::new(0u32);
    let confirm_delete = RwSignal::new(false);
    let editing = RwSignal::new(false);
    let active_tab = RwSignal::new(ActiveTab::Summary);
    // Risk filter — kept at page level so it survives tab switches.
    let risk_filter = RwSignal::new("all".to_string());

    Effect::new(move |_| {
        reload.get();
        let Some(id) = id else {
            state.set(PageState::NotFound);
            return;
        };
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            let contract = match client.get_contract(id).await {
                Ok(c) => c,
                Err(err) if err.is_unauthorized() => {
                    auth.clear();
                    toasts.error("Your session has expired. Please sign in again.");
                    state.set(PageState::Error(err));
                    return;
                }
                Err(ApiError::NotFound { .. }) => {
                    state.set(PageState::NotFound);
                    return;
                }
                Err(err) => {
                    state.set(PageState::Error(err));
                    return;
                }
            };

            let analysis = match client.get_analysis(id).await {
                Ok(a) => a,
                Err(err) => {
                    tracing::warn!(
                        category = err.category(),
                        "analysis fetch failed; showing empty"
                    );
                    AnalysisResponse {
                        contract_id: contract.id,
                        analysis_status: contract.analysis_status.clone(),
                        content_version: contract.content_version,
                        analyzed_content_version: contract.analyzed_content_version,
                        analyzed_at: contract.analyzed_at,
                        analysis_provider: contract.analysis_provider.clone(),
                        analysis_model: contract.analysis_model.clone(),
                        analysis_prompt_version: contract.analysis_prompt_version,
                        analysis_error: contract.analysis_error.clone(),
                        start_date: contract.start_date,
                        end_date: contract.end_date,
                        risk_level: contract.risk_level.clone(),
                        risk_score: contract.risk_score,
                        risk_summary: contract.risk_summary.clone(),
                        risks: Vec::new(),
                        obligations: Vec::new(),
                    }
                }
            };

            state.set(PageState::Loaded {
                contract: Box::new(contract),
                analysis: Box::new(analysis),
            });
        });
    });

    let on_reload = Callback::new(move |_: ()| reload.update(|v| *v += 1));
    let on_delete_request = Callback::new(move |_: ()| confirm_delete.set(true));
    let on_cancel_delete = Callback::new(move |_: ()| confirm_delete.set(false));

    let contract_for_edit = RwSignal::new(None::<ContractResponse>);
    let on_edit_request = Callback::new(move |_: ()| {
        if let PageState::Loaded { contract, .. } = state.get_untracked() {
            contract_for_edit.set(Some(*contract));
        }
        editing.set(true);
    });
    let on_edit_cancel = Callback::new(move |_: ()| editing.set(false));

    let on_delete_confirm = Callback::new(move |_: ()| {
        let Some(id) = id else {
            return;
        };
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            match client.delete_contract(id).await {
                Ok(()) => {
                    toasts.success("Contract deleted.");
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().set_href("/contracts");
                    }
                }
                Err(err) if err.is_unauthorized() => {
                    auth.clear();
                    toasts.error("Your session has expired.");
                }
                Err(err) => toasts.error(err.user_message()),
            }
        });
    });

    view! {
        <AppShell>
            {move || match state.get() {
                PageState::Loading => {
                    view! { <PageLoading label="Loading contract…" /> }.into_any()
                }
                PageState::NotFound => view! {
                    <div class="card">
                        <h2>"Contract not found"</h2>
                        <p class="muted">
                            "This contract does not exist, or you do not have access to it."
                        </p>
                        <a class="btn btn-ghost" href="/contracts">"Back to contracts"</a>
                    </div>
                }.into_any(),
                PageState::Error(err) => view! {
                    <ErrorState message=err.user_message() on_retry=on_reload />
                }.into_any(),
                PageState::Loaded { contract, analysis } => {
                    let contract = *contract;
                    let analysis = *analysis;
                    let contract_for_header = contract.clone();
                    let analysis_for_tabs = analysis.clone();
                    let contract_id = contract.id;
                    let raw_text = contract.raw_text.clone();
                    let obligations = analysis.obligations.clone();
                    let risks_count = analysis.risks.len();
                    let obligations_count = analysis.obligations.len();
                    let status_for_prompt = analysis.analysis_status.clone();

                    view! {
                        <ContractHeader
                            contract=contract_for_header
                            on_reload=on_reload
                            on_delete_request=on_delete_request
                            on_edit_request=on_edit_request
                        />

                        <Show when=move || editing.get()>
                            {move || contract_for_edit.get().map(|c| view! {
                                <ContractEditForm
                                    contract=c
                                    on_saved=on_reload
                                    on_cancel=on_edit_cancel
                                />
                            })}
                        </Show>

                        <AnalysisPrompt
                            status=status_for_prompt
                            contract_id=contract_id
                            on_reload=on_reload
                        />

                        <Tabs
                            tab=active_tab
                            risks_count=risks_count
                            obligations_count=obligations_count
                        />

                        {move || match active_tab.get() {
                            ActiveTab::Summary => view! {
                                <SummaryTab
                                    analysis=analysis_for_tabs.clone()
                                    raw_text=raw_text.clone()
                                />
                            }.into_any(),
                            ActiveTab::Risks => view! {
                                <RisksTab
                                    analysis=analysis_for_tabs.clone()
                                    filter=risk_filter
                                />
                            }.into_any(),
                            ActiveTab::Obligations => view! {
                                <ObligationsTab analysis=analysis_for_tabs.clone() />
                            }.into_any(),
                            ActiveTab::Reminders => view! {
                                <RemindersTab
                                    contract_id=contract_id
                                    obligations=obligations.clone()
                                />
                            }.into_any(),
                        }}

                        <Disclaimer />
                    }.into_any()
                }
            }}

            <ConfirmDialog
                open=confirm_delete.into()
                title="Delete this contract?"
                message="This permanently deletes the contract, its analysis, and all associated reminders. This cannot be undone."
                confirm_label="Delete contract"
                destructive=true
                on_confirm=on_delete_confirm
                on_cancel=on_cancel_delete
            />
        </AppShell>
    }
}

// =========================================================================
// Header
// =========================================================================

#[component]
fn ContractHeader(
    contract: ContractResponse,
    on_reload: Callback<()>,
    on_delete_request: Callback<()>,
    on_edit_request: Callback<()>,
) -> impl IntoView {
    let title = contract.title.clone();
    let id_short = contract
        .id
        .to_string()
        .chars()
        .take(8)
        .collect::<String>();
    let status = contract.analysis_status.clone();
    let risk_level = contract.risk_level.clone();
    let risk_score = contract.risk_score;
    let start = contract
        .start_date
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "—".to_string());
    let end = contract
        .end_date
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "—".to_string());
    let updated = contract.updated_at.format("%Y-%m-%d %H:%M UTC").to_string();
    let error_msg = contract.analysis_error.clone();
    let provider = contract.analysis_provider.clone();
    let model = contract.analysis_model.clone();
    let analyzed_at = contract
        .analyzed_at
        .map(|d| d.format("%Y-%m-%d %H:%M UTC").to_string());

    view! {
        <div class="page-header">
            <div style="min-width: 0;">
                <h1 class="page-title"><SafeText text=title /></h1>
                <p class="page-desc mono text-xs">{format!("ID: {id_short}")}</p>
            </div>
            <div class="page-actions">
                <button
                    class="btn btn-ghost"
                    type="button"
                    on:click=move |_| on_reload.run(())
                >
                    "Refresh"
                </button>
                <button
                    class="btn btn-ghost"
                    type="button"
                    on:click=move |_| on_edit_request.run(())
                >
                    <IconEdit />
                    <span>"Edit"</span>
                </button>
                <button
                    class="btn btn-danger"
                    type="button"
                    on:click=move |_| on_delete_request.run(())
                >
                    <IconTrash />
                    <span>"Delete"</span>
                </button>
            </div>
        </div>

        <div class="card">
            <div class="info-grid">
                <div class="info-block">
                    <div class="info-label">"Analysis"</div>
                    <div class="info-value"><StatusBadge status=status.clone() /></div>
                </div>
                <div class="info-block">
                    <div class="info-label">"Risk level"</div>
                    <div class="info-value">
                        {match risk_level {
                            Some(lvl) => view! { <RiskBadge level=lvl /> }.into_any(),
                            None => {
                                // Analysis completed but no risks → "Low"
                                let lc = status.to_ascii_lowercase();
                                if lc == "completed" {
                                    view! { <span class="badge status-low">"Low"</span> }.into_any()
                                } else {
                                    view! { <span class="muted">"—"</span> }.into_any()
                                }
                            }
                        }}
                    </div>
                </div>
                <div class="info-block">
                    <div class="info-label">"Risk score"</div>
                    <div class="info-value mono">
                        {risk_score
                            .map(|s| format!("{s}/100"))
                            .unwrap_or_else(|| {
                                if status.to_ascii_lowercase() == "completed" {
                                    "0/100".to_string()
                                } else {
                                    "—".to_string()
                                }
                            })}
                    </div>
                </div>
                <div class="info-block">
                    <div class="info-label">"Start date"</div>
                    <div class="info-value mono">{start}</div>
                </div>
                <div class="info-block">
                    <div class="info-label">"End date"</div>
                    <div class="info-value mono">{end}</div>
                </div>
                <div class="info-block">
                    <div class="info-label">"Last updated"</div>
                    <div class="info-value text-xs">{updated}</div>
                </div>
                {provider.map(|p| view! {
                    <div class="info-block">
                        <div class="info-label">"Provider"</div>
                        <div class="info-value text-xs">{p}</div>
                    </div>
                })}
                {model.map(|m| view! {
                    <div class="info-block">
                        <div class="info-label">"Model"</div>
                        <div class="info-value text-xs">{m}</div>
                    </div>
                })}
                {analyzed_at.map(|t| view! {
                    <div class="info-block">
                        <div class="info-label">"Analyzed at"</div>
                        <div class="info-value text-xs">{t}</div>
                    </div>
                })}
            </div>

            {error_msg.map(|msg| view! {
                <div class="alert alert-error mt-4" role="alert">
                    <strong>"Analysis error: "</strong>
                    <SafeText text=msg />
                </div>
            })}
        </div>
    }
}

// =========================================================================
// Analysis prompt (shows when not yet analyzed)
// =========================================================================

#[component]
fn AnalysisPrompt(
    status: String,
    contract_id: Uuid,
    on_reload: Callback<()>,
) -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();
    let is_running = RwSignal::new(false);

    let is_pending = status == "pending";
    let is_analyzed = status == "completed";
    let can_run = !is_pending;

    let on_analyze = move |_| {
        if is_running.get_untracked() {
            return;
        }
        is_running.set(true);
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            match client.analyze_contract(contract_id).await {
                Ok(_) => {
                    toasts.success("Analysis complete.");
                    is_running.set(false);
                    on_reload.run(());
                }
                Err(err) if err.is_unauthorized() => {
                    auth.clear();
                    toasts.error("Your session has expired.");
                    is_running.set(false);
                }
                Err(err) => {
                    toasts.error(err.user_message());
                    is_running.set(false);
                }
            }
        });
    };

    let button_label = if is_analyzed {
        "Re-run analysis"
    } else {
        "Run analysis"
    };

    view! {
        {if is_analyzed {
            view! {
                <div class="row mb-3" style="justify-content: flex-end;">
                    <button
                        class="btn btn-ghost btn-sm"
                        type="button"
                        disabled=move || is_running.get()
                        on:click=on_analyze
                    >
                        <Show when=move || is_running.get() fallback=move || button_label>
                            <Spinner />
                            <span>"Analyzing…"</span>
                        </Show>
                    </button>
                </div>
            }.into_any()
        } else if is_pending {
            view! {
                <div class="alert alert-info mb-3" role="status">
                    <Spinner />
                    <span style="margin-left: 0.5rem;">
                        "Analysis is in progress. This page will not update automatically — click Refresh when it completes."
                    </span>
                </div>
            }.into_any()
        } else {
            view! {
                <div class="card mb-3" style="border-style: dashed;">
                    <div class="row row-between" style="flex-wrap: wrap;">
                        <div>
                            <div class="card-title">"This contract has not been analyzed yet"</div>
                            <p class="hint mt-2" style="margin-bottom: 0;">
                                "Run AI analysis to identify risks and extract obligations."
                            </p>
                        </div>
                        <button
                            class="btn btn-primary"
                            type="button"
                            disabled=move || !can_run || is_running.get()
                            on:click=on_analyze
                        >
                            <Show when=move || is_running.get() fallback=move || button_label>
                                <Spinner />
                                <span>"Analyzing…"</span>
                            </Show>
                        </button>
                    </div>
                </div>
            }.into_any()
        }}
    }
}

// =========================================================================
// Tabs
// =========================================================================

#[component]
fn Tabs(
    tab: RwSignal<ActiveTab>,
    risks_count: usize,
    obligations_count: usize,
) -> impl IntoView {
    let make_class = move |value: ActiveTab| {
        if tab.get() == value {
            "tab tab-active"
        } else {
            "tab"
        }
    };
    let make_aria = move |value: ActiveTab| (tab.get() == value).to_string();

    view! {
        <div class="tabs" role="tablist" aria-label="Contract sections">
            <button
                type="button"
                role="tab"
                class=move || make_class(ActiveTab::Summary)
                aria-selected=move || make_aria(ActiveTab::Summary)
                on:click=move |_| tab.set(ActiveTab::Summary)
            >
                "Summary"
            </button>
            <button
                type="button"
                role="tab"
                class=move || make_class(ActiveTab::Risks)
                aria-selected=move || make_aria(ActiveTab::Risks)
                on:click=move |_| tab.set(ActiveTab::Risks)
            >
                "Risks"
                <span class="tab-count">{risks_count}</span>
            </button>
            <button
                type="button"
                role="tab"
                class=move || make_class(ActiveTab::Obligations)
                aria-selected=move || make_aria(ActiveTab::Obligations)
                on:click=move |_| tab.set(ActiveTab::Obligations)
            >
                "Obligations"
                <span class="tab-count">{obligations_count}</span>
            </button>
            <button
                type="button"
                role="tab"
                class=move || make_class(ActiveTab::Reminders)
                aria-selected=move || make_aria(ActiveTab::Reminders)
                on:click=move |_| tab.set(ActiveTab::Reminders)
            >
                "Reminders"
            </button>
        </div>
    }
}

// =========================================================================
// Summary tab
// =========================================================================

#[component]
fn SummaryTab(analysis: AnalysisResponse, raw_text: String) -> impl IntoView {
    let score = analysis.risk_score.unwrap_or(0);
    let level = analysis.risk_level.clone();
    let donut = risk_donut(score, &level);
    let summary_text = analysis.risk_summary.clone();
    let is_analyzed = analysis.analysis_status == "completed";

    view! {
        {if is_analyzed {
            view! {
                <div class="card">
                    <div class="risk-donut-wrap">
                        <div class="risk-donut" style=donut>
                            <div class="risk-donut-hole">
                                <div class="score">{score}</div>
                                <div class="out-of">"out of 100"</div>
                            </div>
                        </div>
                        <div style="flex: 1; min-width: 200px;">
                            <div class="info-label">"Overall risk assessment"</div>
                            <div style="margin: var(--s-2) 0;">
                                {match level.clone() {
                                    Some(l) => view! { <RiskBadge level=l /> }.into_any(),
                                    None => view! { <span class="badge status-low">"Low"</span> }.into_any(),
                                }}
                            </div>
                            <p class="text-sm muted" style="margin: 0;">
                                {summary_text.unwrap_or_else(|| {
                                    "No material risks identified. This contract uses balanced, market-standard terms.".to_string()
                                })}
                            </p>
                        </div>
                    </div>
                </div>
            }.into_any()
        } else {
            view! { <></> }.into_any()
        }}

        <RawTextSection raw_text=raw_text />
    }
}

fn risk_donut(score: i32, level: &Option<String>) -> String {
    let color = match level.as_deref().unwrap_or("") {
        "critical" => "var(--c-danger)",
        "high" => "#ea580c",
        "medium" => "var(--c-warning)",
        "low" => "var(--c-success)",
        _ => "var(--c-fg-subtle)",
    };
    let pct = score.clamp(0, 100);
    format!(
        "background: conic-gradient({color} 0% {pct}%, var(--c-surface-alt) {pct}% 100%);"
    )
}

#[component]
fn RawTextSection(raw_text: String) -> impl IntoView {
    let empty = raw_text.is_empty();
    let content = if empty { None } else { Some(raw_text) };

    view! {
        {content.map(|text| view! {
            <details class="card mt-4">
                <summary style="cursor: pointer; font-weight: 500;">
                    "Contract text"
                </summary>
                <div class="mt-4" style="max-height: 32rem; overflow-y: auto;">
                    <SafeText text=text pre_wrap=true class="text-sm" />
                </div>
            </details>
        })}
    }
}

// =========================================================================
// Risks tab
// =========================================================================

#[component]
fn RisksTab(analysis: AnalysisResponse, filter: RwSignal<String>) -> impl IntoView {
    let risks = analysis.risks.clone();
    view! { <RisksSection risks=risks filter=filter /> }
}

// =========================================================================
// Obligations tab
// =========================================================================

#[component]
fn ObligationsTab(analysis: AnalysisResponse) -> impl IntoView {
    let obligations = analysis.obligations.clone();
    view! { <ObligationsSection obligations=obligations /> }
}

// =========================================================================
// Reminders tab
// =========================================================================

#[component]
fn RemindersTab(contract_id: Uuid, obligations: Vec<crate::api::models::ObligationItem>) -> impl IntoView {
    view! { <RemindersSection contract_id=contract_id obligations=obligations /> }
}
