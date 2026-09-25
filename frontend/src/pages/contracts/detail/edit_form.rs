//! Contract edit form.
//!
//! Rendered inline as a card on the detail page. Edits `title` and/or
//! `raw_text` via `PATCH /api/contracts/:id`.
//!
//! Editing `raw_text` causes the backend to bump `content_version` and
//! reset `analysis_status` to `not_analyzed`. The parent component
//! refetches after a successful save, which causes this form to
//! disappear and the analysis section to prompt for re-analysis.

use leptos::prelude::*;
use uuid::Uuid;

use crate::api::models::{ContractResponse, UpdateContractRequest};
use crate::auth::context::use_auth;
use crate::components::Spinner;
use crate::state::use_toasts;

const MAX_TITLE_LEN: usize = 500;
const MAX_RAW_TEXT_LEN: usize = 1024 * 1024; // 1 MiB

#[component]
pub fn ContractEditForm(
    contract: ContractResponse,
    on_saved: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();

    let id: Uuid = contract.id;
    let original_title = contract.title.clone();
    let original_raw_text = contract.raw_text.clone();

    let title = RwSignal::new(contract.title.clone());
    let raw_text = RwSignal::new(contract.raw_text.clone());
    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let on_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        if submitting.get_untracked() {
            return;
        }

        let title_now = title.get_untracked();
        let raw_text_now = raw_text.get_untracked();

        let title_trimmed = title_now.trim();
        if title_trimmed.is_empty() {
            error.set(Some("Title is required.".into()));
            return;
        }
        if title_trimmed.len() > MAX_TITLE_LEN {
            error.set(Some(format!(
                "Title must be at most {MAX_TITLE_LEN} characters."
            )));
            return;
        }
        if raw_text_now.trim().is_empty() {
            error.set(Some("Contract text is required.".into()));
            return;
        }
        if raw_text_now.len() > MAX_RAW_TEXT_LEN {
            error.set(Some("Contract text is too large (max 1 MiB).".into()));
            return;
        }

        // Only send fields that actually changed — this keeps the request
        // small and avoids any accidental content_version bumps.
        let title_changed = title_trimmed != original_title.trim();
        let raw_text_changed = raw_text_now != original_raw_text;

        if !title_changed && !raw_text_changed {
            on_cancel.run(());
            return;
        }

        let req = UpdateContractRequest {
            title: if title_changed {
                Some(title_trimmed.to_string())
            } else {
                None
            },
            raw_text: if raw_text_changed {
                Some(raw_text_now)
            } else {
                None
            },
        };

        error.set(None);
        submitting.set(true);

        let auth = auth;
        let toasts = toasts;
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            match client.update_contract(id, &req).await {
                Ok(_) => {
                    toasts.success("Contract updated.");
                    submitting.set(false);
                    on_saved.run(());
                }
                Err(err) if err.is_unauthorized() => {
                    auth.clear();
                    toasts.error("Your session has expired.");
                    submitting.set(false);
                }
                Err(err) => {
                    error.set(Some(err.user_message()));
                    submitting.set(false);
                }
            }
        });
    };

    let content_version = contract.content_version;
    let will_reanalyze = RwSignal::new(raw_text.get_untracked() != contract.raw_text);

    // Recompute the "this will reset analysis" hint whenever raw_text
    // changes.
    let raw_text_now = raw_text;
    let original_raw_text_for_hint = contract.raw_text.clone();
    Effect::new(move |_| {
        let changed = raw_text_now.get() != original_raw_text_for_hint;
        will_reanalyze.set(changed);
    });

    view! {
        <div class="card mt-4">
            <div class="card-header">
                <div class="card-title">"Edit contract"</div>
                <button
                    class="btn btn-ghost btn-sm"
                    type="button"
                    on:click=move |_| on_cancel.run(())
                >
                    "Cancel"
                </button>
            </div>

            <Show when=move || error.get().is_some()>
                <div class="alert alert-error mb-3" role="alert">
                    {move || error.get().unwrap_or_default()}
                </div>
            </Show>

            <form on:submit=on_submit novalidate=true>
                <div class="field">
                    <label for="edit-contract-title">"Title"</label>
                    <input
                        id="edit-contract-title"
                        class="input"
                        type="text"
                        required=true
                        prop:value=move || title.get()
                        on:input=move |ev| title.set(event_target_value(&ev))
                    />
                    <span class="hint">
                        {format!("Up to {MAX_TITLE_LEN} characters.")}
                    </span>
                </div>

                <div class="field">
                    <label for="edit-contract-text">"Contract text"</label>
                    <textarea
                        id="edit-contract-text"
                        class="input"
                        rows="16"
                        required=true
                        prop:value=move || raw_text.get()
                        on:input=move |ev| raw_text.set(event_target_value(&ev))
                    ></textarea>
                    <span class="hint">
                        {move || format!(
                            "{} / {} bytes.",
                            raw_text.get().len(),
                            MAX_RAW_TEXT_LEN,
                        )}
                    </span>
                </div>

                <Show when=move || will_reanalyze.get()>
                    <div class="alert alert-warning" role="note">
                        <strong>"Changing the contract text will invalidate the current analysis. "</strong>
                        "You will need to run AI analysis again to see updated risks and obligations."
                    </div>
                </Show>

                <div class="row row-between mt-4">
                    <span class="text-xs subtle">
                        {format!("Current content version: {content_version}")}
                    </span>
                    <button
                        class="btn btn-primary"
                        type="submit"
                        disabled=move || submitting.get()
                    >
                        <Show when=move || submitting.get() fallback=|| "Save changes">
                            <Spinner />
                            <span>"Saving…"</span>
                        </Show>
                    </button>
                </div>
            </form>
        </div>
    }
}
