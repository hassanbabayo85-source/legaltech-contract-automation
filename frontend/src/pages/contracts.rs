//! `/contracts` — the contract list with client-side filter, search
//! and pagination. The backend exposes paginated listing; to keep
//! filters and search responsive we load the first few pages (capped)
//! and slice locally.

use chrono::{NaiveDate, Utc};
use leptos::prelude::*;

use crate::api::models::ContractSummary;
use crate::api::ApiError;
use crate::auth::context::use_auth;
use crate::components::icons::{IconContracts, IconPlus, IconSearch, IconTrash};
use crate::components::{
    AppShell, ConfirmDialog, EmptyState, ErrorState, ListSkeleton, RiskBadge, SafeText, StatusBadge,
};
use crate::state::use_toasts;

pub mod detail;
pub mod detail_page;
pub mod new_page;

pub use detail_page::ContractDetailPage;
pub use new_page::ContractNewPage;

/// Server page size when fetching. We fetch repeatedly until we have
/// all rows or hit the cap.
const SERVER_FETCH_LIMIT: u32 = 100;
/// Maximum server pages to fetch (caps total at SERVER_FETCH_LIMIT × this).
const MAX_SERVER_PAGES: u32 = 5;
/// Rows shown per client-side page.
const CLIENT_PAGE_SIZE: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lifecycle {
    NoExpiry,
    Active,
    ExpiringSoon,
    Expired,
}

impl Lifecycle {
    fn from_end_date(end: Option<NaiveDate>) -> Self {
        match end {
            None => Lifecycle::NoExpiry,
            Some(d) => {
                let today = Utc::now().date_naive();
                let days = (d - today).num_days();
                if days < 0 {
                    Lifecycle::Expired
                } else if days <= 30 {
                    Lifecycle::ExpiringSoon
                } else {
                    Lifecycle::Active
                }
            }
        }
    }
    fn label(self) -> &'static str {
        match self {
            Lifecycle::NoExpiry => "No expiry",
            Lifecycle::Active => "Active",
            Lifecycle::ExpiringSoon => "Expiring soon",
            Lifecycle::Expired => "Expired",
        }
    }
    fn class(self) -> &'static str {
        match self {
            Lifecycle::NoExpiry => "lifecycle-none",
            Lifecycle::Active => "lifecycle-active",
            Lifecycle::ExpiringSoon => "lifecycle-expiring",
            Lifecycle::Expired => "lifecycle-expired",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Active,
    ExpiringSoon,
    Expired,
}

impl Filter {
    fn matches(self, lc: Lifecycle) -> bool {
        matches!(
            (self, lc),
            (Filter::All, _)
                | (Filter::Active, Lifecycle::Active | Lifecycle::NoExpiry)
                | (Filter::ExpiringSoon, Lifecycle::ExpiringSoon)
                | (Filter::Expired, Lifecycle::Expired)
        )
    }
}

#[component]
pub fn ContractsPage() -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();

    let result = RwSignal::new(None::<Result<Vec<ContractSummary>, ApiError>>);
    let reload = RwSignal::new(0u32);
    let search = RwSignal::new(String::new());
    let filter = RwSignal::new(Filter::All);
    let page = RwSignal::new(1u32);
    let pending_delete = RwSignal::new(None::<uuid::Uuid>);

    Effect::new(move |_| {
        reload.get();
        let auth = auth;
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            let mut all: Vec<ContractSummary> = Vec::new();
            let mut p = 1u32;
            loop {
                let resp = client
                    .list_contracts(p, SERVER_FETCH_LIMIT, Some("created_at"), Some("desc"))
                    .await;
                match resp {
                    Ok(paginated) => {
                        let n = paginated.items.len();
                        let total_so_far = all.len() + n;
                        all.extend(paginated.items);
                        if (n as u32) < SERVER_FETCH_LIMIT
                            || total_so_far as i64 >= paginated.total
                            || p >= MAX_SERVER_PAGES
                        {
                            break;
                        }
                        p += 1;
                    }
                    Err(err) => {
                        if err.is_unauthorized() {
                            auth.clear();
                            toasts.error("Your session has expired. Please sign in again.");
                        }
                        result.set(Some(Err(err)));
                        return;
                    }
                }
            }
            result.set(Some(Ok(all)));
        });
    });

    let on_retry = Callback::new(move |_: ()| reload.update(|v| *v += 1));
    let on_refresh = Callback::new(move |_: ()| {
        page.set(1);
        reload.update(|v| *v += 1);
    });
    let on_cancel_delete = Callback::new(move |_: ()| pending_delete.set(None));

    // Delete handler.
    let on_delete_confirmed = Callback::new(move |_: ()| {
        let Some(id) = pending_delete.get_untracked() else {
            return;
        };
        let auth = auth;
        let toasts = toasts;
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            match client.delete_contract(id).await {
                Ok(()) => {
                    toasts.success("Contract deleted.");
                    pending_delete.set(None);
                    reload.update(|v| *v += 1);
                }
                Err(err) => {
                    toasts.error(err.user_message());
                    pending_delete.set(None);
                }
            }
        });
    });

    view! {
        <AppShell>
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Contracts"</h1>
                    <p class="page-desc">"Manage your legal documents and their AI analysis."</p>
                </div>
                <div class="page-actions">
                    <a class="btn btn-primary" href="/contracts/new">
                        <IconPlus />
                        <span>"New Contract"</span>
                    </a>
                </div>
            </div>

            <ConfirmDialog
                open=Signal::derive(move || pending_delete.get().is_some())
                title="Delete contract?"
                message="This permanently removes the contract and all derived risks, obligations and reminders. It cannot be undone."
                confirm_label="Delete"
                destructive=true
                on_confirm=on_delete_confirmed
                on_cancel=on_cancel_delete
            />

            {move || match result.get() {
                None => view! { <ListSkeleton rows=6 /> }.into_any(),
                Some(Err(err)) => view! {
                    <ErrorState message=err.user_message() on_retry=on_retry />
                }.into_any(),
                Some(Ok(items)) if items.is_empty() => view! {
                    <div class="card">
                        <EmptyState
                            title="No contracts yet"
                            description="Add your first contract to see its AI-identified risks and extract deadlines."
                            icon=ViewFn::from(|| view! { <IconContracts /> }.into_any())
                        >
                            <a class="btn btn-primary" href="/contracts/new">
                                <IconPlus />
                                <span>"Create First Contract"</span>
                            </a>
                        </EmptyState>
                    </div>
                }.into_any(),
                Some(Ok(items)) => {
                    let total = items.len();
                    // Filtered + sorted view (client-side).
                    let q = search.get().trim().to_ascii_lowercase();
                    let f = filter.get();
                    let mut filtered: Vec<ContractSummary> = items
                        .into_iter()
                        .filter(|c| f.matches(Lifecycle::from_end_date(c.end_date)))
                        .filter(|c| {
                            q.is_empty() || c.title.to_ascii_lowercase().contains(&q)
                        })
                        .collect();
                    // Preserve backend sort (created_at desc).
                    filtered.sort_by_key(|c| std::cmp::Reverse(c.created_at));

                    let count_active = filtered
                        .iter()
                        .filter(|c| matches!(Lifecycle::from_end_date(c.end_date), Lifecycle::Active | Lifecycle::NoExpiry))
                        .count();
                    let count_expiring = filtered
                        .iter()
                        .filter(|c| matches!(Lifecycle::from_end_date(c.end_date), Lifecycle::ExpiringSoon))
                        .count();
                    let count_expired = filtered
                        .iter()
                        .filter(|c| matches!(Lifecycle::from_end_date(c.end_date), Lifecycle::Expired))
                        .count();

                    let filtered_total = filtered.len();
                    let current_page = page.get();
                    let last_page = filtered_total.div_ceil(CLIENT_PAGE_SIZE).max(1) as u32;
                    let current_page = current_page.min(last_page).max(1);
                    let start = ((current_page - 1) as usize) * CLIENT_PAGE_SIZE;
                    let end = (start + CLIENT_PAGE_SIZE).min(filtered_total);
                    let page_items: Vec<ContractSummary> = filtered
                        .into_iter()
                        .skip(start)
                        .take(CLIENT_PAGE_SIZE)
                        .collect();

                    let showing_from = if filtered_total == 0 { 0 } else { start + 1 };
                    let showing_line = format!(
                        "Showing {}-{} of {} contracts",
                        showing_from, end, filtered_total
                    );
                    let counts_line = format!(
                        "{} active · {} expiring soon · {} expired",
                        count_active, count_expiring, count_expired
                    );

                    view! {
                        <div class="search-row">
                            <span class="search-icon"><IconSearch /></span>
                            <input
                                class="input"
                                type="search"
                                placeholder="Search contracts by title…"
                                prop:value=move || search.get()
                                on:input=move |ev| {
                                    search.set(event_target_value(&ev));
                                    page.set(1);
                                }
                            />
                        </div>

                        <div class="filters">
                            <FilterPill label="All" current=Filter::All total=total on_pick=filter page=page />
                            <FilterPill label="Active" current=Filter::Active total=count_active on_pick=filter page=page />
                            <FilterPill label="Expiring soon" current=Filter::ExpiringSoon total=count_expiring on_pick=filter page=page />
                            <FilterPill label="Expired" current=Filter::Expired total=count_expired on_pick=filter page=page />
                        </div>

                        <p class="text-xs muted mb-3">{counts_line}</p>

                        <IfEmptyOrTable
                            is_empty=page_items.is_empty()
                            page_items=page_items
                            pending_delete=pending_delete
                        />

                        <div class="pagination">
                            <span class="muted">{showing_line.clone()}</span>
                            <div class="page-buttons">
                                <button
                                    class="page-btn"
                                    type="button"
                                    disabled=move || { page.get() <= 1 }
                                    on:click=move |_| page.update(|p| *p = p.saturating_sub(1).max(1))
                                    aria-label="Previous page"
                                >
                                    "‹"
                                </button>
                                {page_numbers(current_page, last_page).into_iter().map(|slot| {
                                    match slot {
                                        PageSlot::Num(n) => {
                                            let is_active = n == current_page;
                                            view! {
                                                <button
                                                    class=if is_active { "page-btn page-btn-active" } else { "page-btn" }
                                                    type="button"
                                                    on:click=move |_| page.set(n)
                                                >
                                                    {n}
                                                </button>
                                            }.into_any()
                                        }
                                        PageSlot::Ellipsis => view! {
                                            <span class="page-btn" style="border: none; background: transparent; cursor: default;">
                                                "…"
                                            </span>
                                        }.into_any(),
                                    }
                                }).collect_view()}
                                <button
                                    class="page-btn"
                                    type="button"
                                    disabled=move || { page.get() >= last_page }
                                    on:click=move |_| page.update(|p| *p = (*p + 1).min(last_page))
                                    aria-label="Next page"
                                >
                                    "›"
                                </button>
                            </div>
                        </div>

                        <button
                            class="btn btn-ghost btn-sm mt-3"
                            type="button"
                            on:click=move |_| on_refresh.run(())
                        >
                            "Refresh"
                        </button>
                    }.into_any()
                }
            }}
        </AppShell>
    }
}

/// Renders the table, or a "no matches" empty state when the filter
/// yields zero rows (but the raw list was not empty).
#[component]
fn IfEmptyOrTable(
    is_empty: bool,
    page_items: Vec<ContractSummary>,
    pending_delete: RwSignal<Option<uuid::Uuid>>,
) -> impl IntoView {
    if is_empty {
        view! {
            <div class="empty">
                <div class="empty-title">"No contracts match your filters"</div>
                <div class="empty-desc">"Try a different search term or filter."</div>
            </div>
        }
        .into_any()
    } else {
        view! {
            <div class="data-table-wrap">
                <table class="data-table">
                    <thead>
                        <tr>
                            <th scope="col">"Title"</th>
                            <th scope="col">"Analysis"</th>
                            <th scope="col">"Risk"</th>
                            <th scope="col">"Expiry"</th>
                            <th scope="col">"Lifecycle"</th>
                            <th scope="col" style="text-align: right;">"Actions"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {page_items.into_iter().map(|c| view! {
                            <ContractRow contract=c pending_delete=pending_delete />
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        }
        .into_any()
    }
}

#[component]
fn ContractRow(
    contract: ContractSummary,
    pending_delete: RwSignal<Option<uuid::Uuid>>,
) -> impl IntoView {
    let id = contract.id;
    let id_short = id.to_string().chars().take(8).collect::<String>();
    let title = contract.title.clone();
    let href = format!("/contracts/{id}");
    let risk_level = contract.risk_level.clone();
    let analysis_status = contract.analysis_status.clone();
    let end_date = contract.end_date;
    let expiry_text = end_date
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "—".to_string());
    let lc = Lifecycle::from_end_date(end_date);
    let lc_label = lc.label();
    let lc_class = lc.class();

    view! {
        <tr>
            <td>
                <a href=href.clone() class="cell-title">
                    <SafeText text=title />
                </a>
                <div class="cell-id">{id_short}</div>
            </td>
            <td><StatusBadge status=analysis_status /></td>
            <td>
                {risk_level.map(|l| view! { <RiskBadge level=l /> })}
            </td>
            <td class="text-sm muted">{expiry_text}</td>
            <td>
                <span class=format!("badge {lc_class}")>{lc_label}</span>
            </td>
            <td>
                <div class="cell-actions">
                    <a class="icon-btn" href=href.clone() aria-label="Open contract" title="Open">
                        "Open"
                    </a>
                    <button
                        class="icon-btn icon-btn-danger"
                        type="button"
                        aria-label="Delete contract"
                        title="Delete"
                        on:click=move |_| pending_delete.set(Some(id))
                    >
                        <IconTrash />
                    </button>
                </div>
            </td>
        </tr>
    }
}

#[component]
fn FilterPill(
    #[prop(into)] label: String,
    current: Filter,
    total: usize,
    on_pick: RwSignal<Filter>,
    page: RwSignal<u32>,
) -> impl IntoView {
    let cls = move || {
        if on_pick.get() == current {
            "filter-pill filter-pill-active"
        } else {
            "filter-pill"
        }
    };
    view! {
        <button
            class=cls
            type="button"
            on:click=move |_| {
                on_pick.set(current);
                page.set(1);
            }
        >
            {label}
            " "
            <span style="opacity: 0.7;">{format!("({total})")}</span>
        </button>
    }
}

/// One entry in the compact page-number sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageSlot {
    Num(u32),
    Ellipsis,
}

/// Builds a compact page-number sequence: always show first, last, and
/// a window around the current page. `Ellipsis` marks skipped ranges.
fn page_numbers(current: u32, last: u32) -> Vec<PageSlot> {
    if last <= 7 {
        return (1..=last).map(PageSlot::Num).collect();
    }
    let mut out: Vec<PageSlot> = Vec::new();
    let window_start = current.saturating_sub(1).max(2);
    let window_end = (current + 1).min(last - 1);
    out.push(PageSlot::Num(1));
    if window_start > 2 {
        out.push(PageSlot::Ellipsis);
    }
    for n in window_start..=window_end {
        out.push(PageSlot::Num(n));
    }
    if window_end < last - 1 {
        out.push(PageSlot::Ellipsis);
    }
    out.push(PageSlot::Num(last));
    // Dedupe in order.
    let mut dedup: Vec<PageSlot> = Vec::new();
    for slot in out {
        if dedup.last() != Some(&slot) {
            dedup.push(slot);
        }
    }
    dedup
}
