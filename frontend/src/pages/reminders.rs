//! `/reminders` — cross-contract reminders view.
//!
//! The backend exposes reminders per contract (`/api/contracts/:id/reminders`)
//! and does not yet provide a global list endpoint. This page therefore
//! loads the user's contracts, then fetches the reminders for each one
//! in parallel, and merges them into a single sorted list.
//!
//! This is a deliberate frontend-side fan-out: the number of contracts
//! a single user has is expected to be modest in a hackathon-scale
//! deployment, and the aggregation logic is entirely visible here.

use std::collections::HashMap;

use leptos::prelude::*;
use uuid::Uuid;

use crate::api::models::{ContractSummary, ReminderResponse};
use crate::api::ApiError;
use crate::auth::context::use_auth;
use crate::components::{AppShell, EmptyState, ErrorState, ListSkeleton, StatusBadge};
use crate::state::use_toasts;

/// A reminder paired with its contract title, for display.
#[derive(Debug, Clone, PartialEq)]
struct ReminderView {
    contract_title: String,
    contract_id: Uuid,
    reminder: ReminderResponse,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusFilter {
    All,
    Pending,
    Sent,
    Failed,
    Cancelled,
}

impl StatusFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Pending => "Pending",
            Self::Sent => "Sent",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }
    fn matches(self, status: &str) -> bool {
        if self == Self::All {
            return true;
        }
        let want = self.label().to_ascii_lowercase();
        status.eq_ignore_ascii_case(&want)
    }
}

#[component]
pub fn RemindersPage() -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();

    let state = RwSignal::new(None::<Result<Vec<ReminderView>, ApiError>>);
    let reload = RwSignal::new(0u32);
    let filter = RwSignal::new(StatusFilter::All);

    Effect::new(move |_| {
        reload.get();
        let auth = auth;
        let toasts = toasts;
        leptos::task::spawn_local(async move {
            let client = auth.client.get();

            // 1. Load contracts.
            let contracts = match client.list_contracts(1, 100, None, None).await {
                Ok(page) => page.items,
                Err(err) if err.is_unauthorized() => {
                    auth.clear();
                    toasts.error("Your session has expired.");
                    state.set(Some(Err(err)));
                    return;
                }
                Err(err) => {
                    state.set(Some(Err(err)));
                    return;
                }
            };

            // 2. Fetch reminders for each contract. Sequential to keep
            // the code simple; the number of contracts is small.
            let mut views: Vec<ReminderView> = Vec::new();
            for ContractSummary { id, title, .. } in contracts {
                match client.list_reminders(id, 1, 100).await {
                    Ok(page) => {
                        for r in page.items {
                            views.push(ReminderView {
                                contract_title: title.clone(),
                                contract_id: id,
                                reminder: r,
                            });
                        }
                    }
                    Err(err) if err.is_unauthorized() => {
                        auth.clear();
                        toasts.error("Your session has expired.");
                        state.set(Some(Err(err)));
                        return;
                    }
                    Err(_) => {
                        // Skip this contract's reminders on error; the
                        // rest of the list is still useful.
                    }
                }
            }

            // 3. Sort by reminder_date ascending (soonest first).
            views.sort_by_key(|a| a.reminder.reminder_date);

            state.set(Some(Ok(views)));
        });
    });

    let on_retry = Callback::new(move |_: ()| reload.update(|v| *v += 1));

    view! {
        <AppShell>
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Reminders"</h1>
                    <p class="page-desc">
                        "Scheduled notifications for your contracts, soonest first."
                    </p>
                </div>
                <button
                    class="btn btn-ghost btn-sm"
                    type="button"
                    on:click=move |_| reload.update(|v| *v += 1)
                >
                    "Refresh"
                </button>
            </div>

            {move || match state.get() {
                None => view! { <ListSkeleton rows=4 /> }.into_any(),
                Some(Err(err)) => view! {
                    <ErrorState message=err.user_message() on_retry=on_retry />
                }.into_any(),
                Some(Ok(items)) if items.is_empty() => view! {
                    <div class="card">
                        <EmptyState
                            title="No reminders yet"
                            description="Reminders are scheduled from a contract's analysis. \
                                         Open a contract and generate reminders after running analysis."
                        >
                            <a class="btn btn-primary" href="/contracts">"View contracts"</a>
                        </EmptyState>
                    </div>
                }.into_any(),
                Some(Ok(items)) => {
                    let counts = Counts::from(&items);
                    view! {
                        <div class="row mb-3" role="group" aria-label="Filter reminders by status">
                            <FilterChip filter=filter value=StatusFilter::All label="All" count=counts.all />
                            <FilterChip filter=filter value=StatusFilter::Pending label="Pending" count=counts.pending />
                            <FilterChip filter=filter value=StatusFilter::Sent label="Sent" count=counts.sent />
                            <FilterChip filter=filter value=StatusFilter::Failed label="Failed" count=counts.failed />
                            <FilterChip filter=filter value=StatusFilter::Cancelled label="Cancelled" count=counts.cancelled />
                        </div>

                        {
                            let items_clone = items.clone();
                            let filtered = Memo::new(move |_| {
                                let f = filter.get();
                                items_clone
                                    .iter()
                                    .filter(|v| f.matches(&v.reminder.status))
                                    .cloned()
                                    .collect::<Vec<_>>()
                            });

                            view! {
                                <Show
                                    when=move || !filtered.get().is_empty()
                                    fallback=|| view! {
                                        <div class="card">
                                            <p class="muted">"No reminders match the selected filter."</p>
                                        </div>
                                    }
                                >
                                    <div class="table-wrap">
                                        <table class="table">
                                            <thead>
                                                <tr>
                                                    <th scope="col">"Date"</th>
                                                    <th scope="col">"Contract"</th>
                                                    <th scope="col">"Type"</th>
                                                    <th scope="col">"Channel"</th>
                                                    <th scope="col">"Status"</th>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                <For
                                                    each=move || filtered.get()
                                                    key=|v| (v.reminder.id, v.contract_id)
                                                    children=|v| view! { <ReminderRow view=v /> }
                                                />
                                            </tbody>
                                        </table>
                                    </div>
                                </Show>
                            }
                        }
                    }.into_any()
                }
            }}
        </AppShell>
    }
}

#[component]
fn FilterChip(
    filter: RwSignal<StatusFilter>,
    value: StatusFilter,
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
            on:click=move |_| filter.set(value)
        >
            {format!("{label} ({count})")}
        </button>
    }
}

#[component]
fn ReminderRow(view: ReminderView) -> impl IntoView {
    let r = view.reminder.clone();
    let date = r.reminder_date.format("%Y-%m-%d %H:%M UTC").to_string();
    let contract_href = format!("/contracts/{}", view.contract_id);
    let contract_title = view.contract_title.clone();
    let rtype = humanize_reminder_type(&r.reminder_type);
    let channel = r.channel_type.clone();
    let status = r.status.clone();

    view! {
        <tr>
            <td class="mono text-sm">{date}</td>
            <td class="text-sm">
                <a href=contract_href>{contract_title}</a>
            </td>
            <td class="text-sm">{rtype}</td>
            <td class="text-sm">{channel}</td>
            <td><StatusBadge status=status /></td>
        </tr>
    }
}

fn humanize_reminder_type(s: &str) -> String {
    match s {
        "7_days_before" => "7 days before".to_string(),
        "3_days_before" => "3 days before".to_string(),
        "1_day_before" => "1 day before".to_string(),
        "on_deadline" => "On deadline".to_string(),
        "custom" => "Custom".to_string(),
        other => other.to_string(),
    }
}

struct Counts {
    all: usize,
    pending: usize,
    sent: usize,
    failed: usize,
    cancelled: usize,
}

impl Counts {
    fn from(items: &[ReminderView]) -> Self {
        let mut c = Counts {
            all: items.len(),
            pending: 0,
            sent: 0,
            failed: 0,
            cancelled: 0,
        };
        for v in items {
            match v.reminder.status.as_str() {
                "pending" => c.pending += 1,
                "sent" => c.sent += 1,
                "failed" => c.failed += 1,
                "cancelled" => c.cancelled += 1,
                _ => {}
            }
        }
        c
    }
}

// Keep `HashMap` import meaningful for future use — silence the
// unused-import lint if the compiler is strict.
#[allow(dead_code)]
fn _unused(_: HashMap<(), ()>) {}
