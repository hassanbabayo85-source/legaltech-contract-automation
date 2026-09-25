//! `/` — the dashboard home.
//!
//! All numbers come from `GET /api/dashboard/summary`. No placeholder
//! statistics. Where the UI cannot be derived from that summary alone
//! (e.g. a list of upcoming reminders — the API is contract-scoped),
//! we link to the relevant page instead of inventing data.

use leptos::prelude::*;

use crate::api::models::DashboardSummary;
use crate::api::ApiError;
use crate::auth::context::use_auth;
use crate::components::icons::{IconBolt, IconChannels, IconCheck, IconClock, IconPlus};
use crate::components::{AppShell, EmptyState, ErrorState, PageLoading};
use crate::state::use_toasts;

#[component]
pub fn DashboardPage() -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();

    let summary = RwSignal::new(None::<Result<DashboardSummary, ApiError>>);
    let reload_trigger = RwSignal::new(0u32);

    Effect::new(move |_| {
        reload_trigger.get();
        let auth = auth;
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            let result = client.dashboard_summary().await;
            if let Err(ApiError::Unauthorized { .. }) = &result {
                auth.clear();
                toasts.error("Your session has expired. Please sign in again.");
            }
            summary.set(Some(result));
        });
    });

    let on_retry = Callback::new(move |_| reload_trigger.update(|v| *v += 1));

    view! {
        <AppShell>
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Dashboard"</h1>
                    <p class="page-desc">"Here's what's happening with your contracts."</p>
                </div>
                <div class="page-actions">
                    <a class="btn btn-primary" href="/contracts/new">
                        <IconPlus />
                        <span>"New Contract"</span>
                    </a>
                </div>
            </div>

            {move || match summary.get() {
                None => view! { <PageLoading label="Loading dashboard…" /> }.into_any(),
                Some(Err(err)) => view! {
                    <ErrorState message=err.user_message() on_retry=on_retry />
                }.into_any(),
                Some(Ok(s)) => {
                    let empty = s.contracts.total == 0
                        && s.reminders.pending == 0
                        && s.channels.enabled == 0;
                    if empty {
                        view! { <EmptyDashboard/> }.into_any()
                    } else {
                        view! { <DashboardBody summary=s /> }.into_any()
                    }
                }
            }}

            <Disclaimer />
        </AppShell>
    }
}

#[component]
fn DashboardBody(summary: DashboardSummary) -> impl IntoView {
    let s = summary;

    // Pre-compute every string so the `view!` macro sees plain values.
    let total_contracts = s.contracts.total.to_string();
    let pending_reminders = s.reminders.pending.to_string();
    let analyses = s.contracts.analyzed.to_string();
    let total_reminders = s.reminders.pending + s.reminders.sent + s.reminders.failed;

    let analyzed_hint = format!("{} analyzed", s.contracts.analyzed);
    let reminders_hint = format!("{} due in 30 days", s.reminders.upcoming_30d);
    let analyses_hint = format!("{} pending", s.contracts.pending_analysis);

    let attempted = s.reminders.sent + s.reminders.failed;
    let delivery_value = if attempted > 0 {
        format!("{}%", (s.reminders.sent * 100) / attempted)
    } else {
        "—".to_string()
    };
    let delivery_hint = if attempted > 0 {
        format!("{} sent, {} failed", s.reminders.sent, s.reminders.failed)
    } else {
        "No attempts yet".to_string()
    };

    let donut_style_str = donut_style(s.reminders.pending, s.reminders.sent, s.reminders.failed);
    let total_reminders_str = total_reminders.to_string();

    let analyzed_pair = format!("{} / {}", s.contracts.analyzed, s.contracts.total);
    let analysed_pct: i64 = if s.contracts.total > 0 {
        (s.contracts.analyzed * 100) / s.contracts.total
    } else {
        0
    };
    let progress_style = format!("width: {analysed_pct}%;");

    view! {
        <div class="stat-grid">
            <div class="stat-card stat-info">
                <div class="stat-label">"Total contracts"</div>
                <div class="stat-value">{total_contracts}</div>
                <div class="hint">{analyzed_hint}</div>
            </div>
            <div class="stat-card stat-warn">
                <div class="stat-label">"Pending reminders"</div>
                <div class="stat-value">{pending_reminders}</div>
                <div class="hint">{reminders_hint}</div>
            </div>
            <div class="stat-card stat-info">
                <div class="stat-label">"AI analyses"</div>
                <div class="stat-value">{analyses}</div>
                <div class="hint">{analyses_hint}</div>
            </div>
            <div class="stat-card stat-success">
                <div class="stat-label">"Delivery success"</div>
                <div class="stat-value">{delivery_value}</div>
                <div class="hint">{delivery_hint}</div>
            </div>
        </div>

        <div class="dashboard-grid">
            <div class="card">
                <div class="card-header">
                    <div class="card-title">"Reminder delivery"</div>
                    <a class="text-sm" href="/reminders">"View all"</a>
                </div>
                <div class="donut-wrap">
                    <div class="donut" style=donut_style_str>
                        <div class="donut-hole">
                            <strong>{total_reminders_str}</strong>
                            <small>"total"</small>
                        </div>
                    </div>
                    <div class="donut-legend">
                        <div class="donut-legend-item">
                            <span>
                                <span class="dot" style="background: var(--c-success);"></span>
                                "Sent"
                            </span>
                            <strong>{s.reminders.sent}</strong>
                        </div>
                        <div class="donut-legend-item">
                            <span>
                                <span class="dot" style="background: var(--c-warning);"></span>
                                "Pending"
                            </span>
                            <strong>{s.reminders.pending}</strong>
                        </div>
                        <div class="donut-legend-item">
                            <span>
                                <span class="dot" style="background: var(--c-danger);"></span>
                                "Failed"
                            </span>
                            <strong>{s.reminders.failed}</strong>
                        </div>
                    </div>
                </div>
            </div>

            <div class="stack">
                <div class="card">
                    <div class="card-header">
                        <div class="card-title">"Contract analysis"</div>
                    </div>
                    <div class="stack stack-sm">
                        <div class="row row-between text-sm">
                            <span class="muted">"Analyzed"</span>
                            <strong>{analyzed_pair}</strong>
                        </div>
                        <div class="progress">
                            <div class="progress-fill" style=progress_style></div>
                        </div>
                        <div class="row row-between text-sm">
                            <span class="muted">"High or critical"</span>
                            <strong>{s.contracts.high_or_critical}</strong>
                        </div>
                        <div class="row row-between text-sm">
                            <span class="muted">"Overdue obligations"</span>
                            <strong>{s.obligations.overdue}</strong>
                        </div>
                    </div>
                </div>

                <div class="card">
                    <div class="card-header">
                        <div class="card-title">"Quick actions"</div>
                    </div>
                    <div class="stack stack-sm">
                        <a class="btn btn-ghost" href="/contracts/new">
                            <IconPlus />
                            <span>"Add contract"</span>
                        </a>
                        <a class="btn btn-ghost" href="/notification-channels">
                            <IconChannels />
                            <span>"Manage channels"</span>
                        </a>
                        <a class="btn btn-ghost" href="/reminders">
                            <IconClock />
                            <span>"View reminders"</span>
                        </a>
                    </div>
                </div>
            </div>
        </div>

        <div class="feature-strip mt-5">
            <div class="feature">
                <span class="feature-icon"><IconBolt /></span>
                <div>
                    <div class="feature-title">"AI analysis"</div>
                    <div class="feature-desc">"Extract risks and obligations automatically."</div>
                </div>
            </div>
            <div class="feature">
                <span class="feature-icon"><IconClock /></span>
                <div>
                    <div class="feature-title">"Deadline reminders"</div>
                    <div class="feature-desc">"7/3/1/0-day schedule, per deadline."</div>
                </div>
            </div>
            <div class="feature">
                <span class="feature-icon"><IconCheck /></span>
                <div>
                    <div class="feature-title">"Delivery tracking"</div>
                    <div class="feature-desc">"At-least-once dispatch with retries."</div>
                </div>
            </div>
        </div>
    }
}

/// Builds a conic-gradient string for the reminder donut.
///
/// When there are no reminders at all, we deliberately render a flat
/// muted circle instead of a coloured arc — a full coloured ring at
/// 0 total would suggest data that is not there.
fn donut_style(pending: i64, sent: i64, failed: i64) -> String {
    let total_i = pending + sent + failed;
    if total_i == 0 {
        return "background: var(--c-surface-alt);".to_string();
    }
    let total = total_i as f64;
    let p_sent = sent as f64 / total * 100.0;
    let p_failed = failed as f64 / total * 100.0;
    let p_pending = pending as f64 / total * 100.0;
    let a = p_sent;
    let b = p_sent + p_failed;
    let c = b + p_pending;
    format!(
        "background: conic-gradient(var(--c-success) 0% {a}%, \
         var(--c-danger) {a}% {b}%, \
         var(--c-warning) {b}% {c}%);"
    )
}

#[component]
fn EmptyDashboard() -> impl IntoView {
    view! {
        <div class="card">
            <EmptyState
                title="Welcome to LexGuard"
                description="Add your first contract to see its AI-identified risks and extract deadlines. \
                             You can then schedule reminders so nothing slips."
            >
                <a class="btn btn-primary" href="/contracts/new">
                    <IconPlus />
                    <span>"Create First Contract"</span>
                </a>
            </EmptyState>
        </div>
    }
}

/// The legal disclaimer, shown on every analysis-bearing page.
#[component]
pub fn Disclaimer() -> impl IntoView {
    view! {
        <div class="disclaimer mt-4" role="note">
            <strong>"Not legal advice. "</strong>
            "LexGuard provides AI-assisted contract risk analysis and deadline management. \
             It does not provide legal advice and does not replace a qualified legal professional."
        </div>
    }
}
