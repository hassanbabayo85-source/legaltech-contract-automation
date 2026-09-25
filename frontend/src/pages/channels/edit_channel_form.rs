//! Channel edit form.
//!
//! Lets the owner rename a channel and toggle its `enabled` flag.
//! Deliberately does **not** expose the credential/config fields —
//! those are secrets that the backend never returns, so the frontend
//! cannot pre-fill them. Replacing a credential is done by deleting
//! and recreating the channel; a dedicated "rotate credential" flow is
//! a future improvement.

use leptos::prelude::*;
use serde_json::json;
use uuid::Uuid;

use crate::api::models::ChannelResponse;
use crate::auth::context::use_auth;
use crate::components::Spinner;
use crate::state::use_toasts;

const MAX_NAME_LEN: usize = 100;

#[component]
pub fn EditChannelForm(
    channel: ChannelResponse,
    on_saved: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();

    let id: Uuid = channel.id;
    let channel_type = channel.channel_type.clone();

    let name = RwSignal::new(channel.name.clone());
    let enabled = RwSignal::new(channel.enabled);
    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let on_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        if submitting.get_untracked() {
            return;
        }

        let name_trimmed = name.get_untracked().trim().to_string();
        if name_trimmed.is_empty() {
            error.set(Some("Name is required.".into()));
            return;
        }
        if name_trimmed.len() > MAX_NAME_LEN {
            error.set(Some(format!(
                "Name must be at most {MAX_NAME_LEN} characters."
            )));
            return;
        }

        let body = json!({
            "name": name_trimmed,
            "enabled": enabled.get_untracked(),
        });

        error.set(None);
        submitting.set(true);

        let auth = auth;
        let toasts = toasts;
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            match client.update_channel(id, &body).await {
                Ok(_) => {
                    toasts.success("Channel updated.");
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

    view! {
        <div class="card mt-4">
            <div class="card-header">
                <div class="card-title">
                    {format!("Edit channel — {channel_type}")}
                </div>
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
                    <label for="edit-channel-name">"Name"</label>
                    <input
                        id="edit-channel-name"
                        class="input"
                        type="text"
                        required=true
                        prop:value=move || name.get()
                        on:input=move |ev| name.set(event_target_value(&ev))
                    />
                </div>

                <div class="field">
                    <label>
                        <input
                            type="checkbox"
                            prop:checked=move || enabled.get()
                            on:change=move |ev| enabled.set(event_target_checked(&ev))
                        />
                        " Enabled (the worker can deliver reminders through this channel)"
                    </label>
                    <span class="hint">
                        "Disabled channels are skipped by the reminder worker. \
                         At most one enabled channel per type is allowed."
                    </span>
                </div>

                <div class="disclaimer mt-3" role="note">
                    <strong>"Credentials cannot be edited here. "</strong>
                    "Channel credentials are stored encrypted and never returned by the API. \
                     To replace a credential, delete this channel and create a new one."
                </div>

                <div class="row row-between mt-4">
                    <span class="text-xs subtle">
                        "Only one enabled channel per type is allowed."
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
