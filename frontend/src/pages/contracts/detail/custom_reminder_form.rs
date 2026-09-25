//! Custom reminder creation form.
//!
//! Creates a `POST /api/contracts/:id/reminders` request. The optional
//! `obligation_id` can be chosen from the analysis obligations. The
//! channel must be one of the user's configured enabled channels.
//!
//! # Date/time input
//!
//! `<input type="datetime-local">` gives a naive local datetime string
//! like `2026-10-20T09:00`. The backend expects an RFC 3339 `DateTime<Utc>`.
//! We interpret the entered value as **UTC** and append `:00Z`. This is
//! a known UX limitation (no timezone picker) documented in
//! `docs/FRONTEND.md`.

use leptos::prelude::*;
use uuid::Uuid;

use crate::api::models::{ChannelResponse, CreateCustomReminderRequest, ObligationItem};
use crate::auth::context::use_auth;
use crate::components::{EmptyState, Spinner};
use crate::state::use_toasts;

#[component]
pub fn CustomReminderForm(
    contract_id: Uuid,
    obligations: Vec<ObligationItem>,
    channels: Vec<ChannelResponse>,
    on_created: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();

    let obligation_id = RwSignal::new(String::new()); // "" means none
    let reminder_date = RwSignal::new(String::new());
    let channel_type = RwSignal::new(
        channels
            .first()
            .map(|c| c.channel_type.clone())
            .unwrap_or_default(),
    );
    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let enabled_channels: Vec<ChannelResponse> =
        channels.into_iter().filter(|c| c.enabled).collect();

    if enabled_channels.is_empty() {
        return view! {
            <div class="card mt-4">
                <div class="card-header">
                    <div class="card-title">"Add custom reminder"</div>
                    <button
                        class="btn btn-ghost btn-sm"
                        type="button"
                        on:click=move |_| on_cancel.run(())
                    >
                        "Cancel"
                    </button>
                </div>
                <EmptyState
                    title="No enabled notification channels"
                    description="Configure and enable a channel before creating a reminder."
                >
                    <a class="btn btn-primary" href="/notification-channels">
                        "Manage channels"
                    </a>
                </EmptyState>
            </div>
        }
        .into_any();
    }

    let channel_options = enabled_channels
        .iter()
        .map(|c| {
            let value = c.channel_type.clone();
            let label = format!("{} ({})", c.name, c.channel_type);
            (value, label)
        })
        .collect::<Vec<_>>();

    let obligation_options = obligations
        .iter()
        .map(|o| {
            let value = o.id.to_string();
            let label = format!(
                "{} (due {})",
                o.title,
                o.due_date
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| "—".to_string())
            );
            (value, label)
        })
        .collect::<Vec<_>>();

    let on_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        if submitting.get_untracked() {
            return;
        }

        let date_input = reminder_date.get_untracked().trim().to_string();
        if date_input.is_empty() {
            error.set(Some("Reminder date is required.".into()));
            return;
        }

        // `datetime-local` produces `YYYY-MM-DDTHH:MM`. We interpret it
        // as UTC and append seconds + Z.
        let reminder_date_utc = if date_input.len() == 16 {
            format!("{date_input}:00Z")
        } else if date_input.ends_with('Z') {
            date_input.clone()
        } else {
            format!("{date_input}Z")
        };

        let channel = channel_type.get_untracked();
        if channel.is_empty() {
            error.set(Some("Channel is required.".into()));
            return;
        }

        let ob_id_str = obligation_id.get_untracked();
        let ob_id = if ob_id_str.is_empty() {
            None
        } else {
            match Uuid::parse_str(&ob_id_str) {
                Ok(id) => Some(id),
                Err(_) => {
                    error.set(Some("Invalid obligation selection.".into()));
                    return;
                }
            }
        };

        // Parse the assembled string via chrono. chrono's DateTime
        // deserializer accepts RFC 3339, which is what we built.
        let parsed_date: chrono::DateTime<chrono::Utc> =
            match reminder_date_utc.parse::<chrono::DateTime<chrono::Utc>>() {
                Ok(d) => d,
                Err(_) => {
                    error.set(Some("Reminder date is not a valid date.".into()));
                    return;
                }
            };

        let req = CreateCustomReminderRequest {
            obligation_id: ob_id,
            reminder_date: parsed_date,
            channel_type: channel,
        };

        error.set(None);
        submitting.set(true);

        let auth = auth;
        let toasts = toasts;
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            match client.create_custom_reminder(contract_id, &req).await {
                Ok(_) => {
                    toasts.success("Reminder scheduled.");
                    submitting.set(false);
                    on_created.run(());
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
                <div class="card-title">"Add custom reminder"</div>
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
                    <label for="custom-reminder-obligation">"Obligation (optional)"</label>
                    <select
                        id="custom-reminder-obligation"
                        class="input"
                        on:change=move |ev| obligation_id.set(event_target_value(&ev))
                    >
                        <option value="" selected=move || obligation_id.get().is_empty()>
                            "— No specific obligation —"
                        </option>
                        {obligation_options.into_iter().map(|(value, label)| {
                            view! { <option value=value>{label}</option> }
                        }).collect_view()}
                    </select>
                    <span class="hint">"Attach the reminder to a specific obligation, or leave blank for a contract-level reminder."</span>
                </div>

                <div class="field">
                    <label for="custom-reminder-date">"Reminder date and time (UTC)"</label>
                    <input
                        id="custom-reminder-date"
                        class="input"
                        type="datetime-local"
                        required=true
                        prop:value=move || reminder_date.get()
                        on:input=move |ev| reminder_date.set(event_target_value(&ev))
                    />
                    <span class="hint">
                        "Entered time is interpreted as UTC. There is no timezone picker yet."
                    </span>
                </div>

                <div class="field">
                    <label for="custom-reminder-channel">"Notification channel"</label>
                    <select
                        id="custom-reminder-channel"
                        class="input"
                        on:change=move |ev| channel_type.set(event_target_value(&ev))
                    >
                        {channel_options.into_iter().map(|(value, label)| {
                            view! { <option value=value>{label}</option> }
                        }).collect_view()}
                    </select>
                </div>

                <div class="row row-between mt-4">
                    <span class="text-xs subtle">
                        "Reminders are dispatched by the background worker."
                    </span>
                    <button
                        class="btn btn-primary"
                        type="submit"
                        disabled=move || submitting.get()
                    >
                        <Show when=move || submitting.get() fallback=|| "Schedule reminder">
                            <Spinner />
                            <span>"Scheduling…"</span>
                        </Show>
                    </button>
                </div>
            </form>
        </div>
    }
    .into_any()
}
