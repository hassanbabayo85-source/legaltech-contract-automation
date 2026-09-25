//! Loading, empty, and error presentation helpers.

use leptos::prelude::*;

#[component]
pub fn Spinner() -> impl IntoView {
    view! { <span class="spinner" aria-hidden="true"></span> }
}

#[component]
pub fn PageLoading(#[prop(optional, into)] label: Option<String>) -> impl IntoView {
    let label = label.unwrap_or_else(|| "Loading…".to_string());
    view! {
        <div role="status" aria-live="polite" class="card">
            <div class="row">
                <Spinner />
                <span class="muted">{label}</span>
            </div>
        </div>
    }
}

#[component]
pub fn ListSkeleton(#[prop(default = 4)] rows: usize) -> impl IntoView {
    let items: Vec<_> = (0..rows)
        .map(|_| view! { <div class="skeleton block"></div> })
        .collect();
    view! { <div aria-hidden="true">{items}</div> }
}

#[component]
pub fn EmptyState(
    #[prop(into)] title: String,
    #[prop(into)] description: String,
    /// Optional icon (e.g. `view! { <IconContracts /> }`) shown above
    /// the title, inside a soft-coloured circle.
    #[prop(optional)]
    icon: Option<ViewFn>,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    view! {
        <div class="empty">
            {icon.map(|i| view! {
                <div class="empty-icon" aria-hidden="true">
                    {i.run()}
                </div>
            })}
            <div class="empty-title">{title}</div>
            <div class="empty-desc">{description}</div>
            {children.map(|c| c())}
        </div>
    }
}

#[component]
pub fn ErrorState(
    #[prop(into)] message: String,
    #[prop(optional)] on_retry: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <div class="alert alert-error" role="alert">
            <div class="row row-between">
                <span>{message}</span>
                {on_retry.map(|cb| view! {
                    <button class="btn btn-ghost btn-sm" type="button" on:click=move |_| cb.run(())>
                        "Try again"
                    </button>
                })}
            </div>
        </div>
    }
}
