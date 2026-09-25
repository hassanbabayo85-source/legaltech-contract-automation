//! Reminders section on the contract detail page.
//!
//! Lets the owner:
//!
//! * view existing reminders for the contract
//! * regenerate system reminders (idempotent server-side)
//! * add a custom reminder (form)
//! * cancel a pending reminder (with confirmation)

use leptos::prelude::*;
use uuid::Uuid;

use crate::api::models::{ChannelResponse, ObligationItem, ReminderResponse};
use crate::api::ApiError;
use crate::auth::context::use_auth;
use crate::components::{ConfirmDialog, ErrorState, ListSkeleton, Spinner, StatusBadge};
use crate::pages::contracts::detail::CustomReminderForm;
use crate::state::use_toasts;

#[component]
pub fn RemindersSection(contract_id: Uuid, obligations: Vec<ObligationItem>) -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();

    // Store obligations so the closure inside `<Show>` (which must be
    // `Fn`, not `FnOnce`) can clone them on every render without moving
    // the original vector out of scope.
    let obligations_store = StoredValue::new(obligations);

    let reminders = RwSignal::new(None::<Result<Vec<ReminderResponse>, ApiError>>);
    let channels = RwSignal::new(Vec::<ChannelResponse>::new());
    let reload = RwSignal::new(0u32);
    let confirm_cancel_id = RwSignal::new(None::<Uuid>);
    let cancel_dialog_open = RwSignal::new(false);
    let on_cancel_cancel_dialog = Callback::new(move |_: ()| {
        cancel_dialog_open.set(false);
        confirm_cancel_id.set(None);
    });
    let generating = RwSignal::new(false);
    let show_custom = RwSignal::new(false);

    // Load reminders.
    Effect::new(move |_| {
        reload.get();
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            match client.list_reminders(contract_id, 1, 100).await {
                Ok(paginated) => reminders.set(Some(Ok(paginated.items))),
                Err(err) if err.is_unauthorized() => {
                    auth.clear();
                    toasts.error("Your session has expired.");
                    reminders.set(Some(Err(err)));
                }
                Err(err) => reminders.set(Some(Err(err))),
            }
        });
    });

    // Load channels for the custom-reminder form (once).
    Effect::new(move |_| {
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            if let Ok(page) = client.list_channels(1, 100).await {
                channels.set(page.items);
            }
        });
    });

    let on_reload = Callback::new(move |_: ()| reload.update(|v| *v += 1));
    let on_custom_created = Callback::new(move |_: ()| {
        show_custom.set(false);
        reload.update(|v| *v += 1);
    });
    let on_custom_cancel = Callback::new(move |_: ()| show_custom.set(false));

    let on_generate = {
        move |_| {
            if generating.get_untracked() {
                return;
            }
            generating.set(true);
            let toasts = toasts;
            let auth = auth;
            leptos::task::spawn_local(async move {
                let client = auth.client.get();
                match client.generate_reminders(contract_id).await {
                    Ok(gen) => {
                        toasts.success(format!(
                            "Reminders generated: {} created, {} preserved.",
                            gen.created, gen.preserved_custom
                        ));
                        generating.set(false);
                        on_reload.run(());
                    }
                    Err(err) if err.is_unauthorized() => {
                        auth.clear();
                        toasts.error("Your session has expired.");
                        generating.set(false);
                    }
                    Err(err) => {
                        toasts.error(err.user_message());
                        generating.set(false);
                    }
                }
            });
        }
    };

    let on_cancel_confirm = Callback::new({
        move |_: ()| {
            let Some(id) = confirm_cancel_id.get_untracked() else {
                return;
            };
            let toasts = toasts;
            let auth = auth;
            leptos::task::spawn_local(async move {
                let client = auth.client.get();
                match client.cancel_reminder(id).await {
                    Ok(_) => {
                        toasts.success("Reminder cancelled.");
                        on_reload.run(());
                    }
                    Err(err) if err.is_unauthorized() => {
                        auth.clear();
                        toasts.error("Your session has expired.");
                    }
                    Err(err) => toasts.error(err.user_message()),
                }
            });
            confirm_cancel_id.set(None);
            cancel_dialog_open.set(false);
        }
    });

    view! {
        <section class="card mt-4" aria-labelledby="reminders-heading">
            <div class="card-header">
                <div class="card-title" id="reminders-heading">"Reminders"</div>
                <div class="btn-row">
                    <button
                        class="btn btn-ghost btn-sm"
                        type="button"
                        on:click=move |_| show_custom.set(true)
                        disabled=move || show_custom.get()
                    >
                        "Add custom"
                    </button>
                    <button
                        class="btn btn-ghost btn-sm"
                        type="button"
                        disabled=move || generating.get()
                        on:click=on_generate
                    >
                        <Show when=move || generating.get() fallback=|| "Generate reminders">
                            <Spinner />
                            <span>"Generating…"</span>
                        </Show>
                    </button>
                </div>
            </div>

            <p class="text-xs subtle mb-3">
                "Reminders are scheduled from the contract's analysis. \
                 Generation is idempotent: re-running it will not duplicate existing reminders."
            </p>

            <Show when=move || show_custom.get()>
                {move || view! {
                    <CustomReminderForm
                        contract_id=contract_id
                        obligations=obligations_store.get_value()
                        channels=channels.get()
                        on_created=on_custom_created
                        on_cancel=on_custom_cancel
                    />
                }}
            </Show>

            {move || match reminders.get() {
                None => view! { <ListSkeleton rows=2 /> }.into_any(),
                Some(Err(err)) => view! {
                    <ErrorState message=err.user_message() on_retry=on_reload />
                }.into_any(),
                Some(Ok(items)) if items.is_empty() => view! {
                    <p class="muted">
                        "No reminders scheduled yet. Generate them after the analysis is complete, \
                         or add a custom reminder."
                    </p>
                }.into_any(),
                Some(Ok(items)) => {
                    let total = items.len();
                    view! {
                        <p class="text-sm muted mb-2">{format!("{total} reminder(s)")}</p>
                        <div class="table-wrap">
                            <table class="table">
                                <thead>
                                    <tr>
                                        <th scope="col">"Date"</th>
                                        <th scope="col">"Type"</th>
                                        <th scope="col">"Channel"</th>
                                        <th scope="col">"Status"</th>
                                        <th scope="col">"Source"</th>
                                        <th scope="col"><span class="subtle">"Actions"</span></th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {items.into_iter().map(|r| view! {
                                        <ReminderRow
                                            reminder=r
                                            on_cancel_request=Callback::new(move |id| {
                                                confirm_cancel_id.set(Some(id));
                                                cancel_dialog_open.set(true);
                                            })
                                        />
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }
            }}

            <ConfirmDialog
                open=cancel_dialog_open.into()
                title="Cancel this reminder?"
                message="The reminder will not be sent. This cannot be undone, but you can regenerate reminders from a fresh analysis."
                confirm_label="Cancel reminder"
                cancel_label="Keep it"
                destructive=true
                on_cancel=on_cancel_cancel_dialog
                on_confirm=on_cancel_confirm
            />
        </section>
    }
}

#[component]
fn ReminderRow(reminder: ReminderResponse, on_cancel_request: Callback<Uuid>) -> impl IntoView {
    let id = reminder.id;
    let date = reminder
        .reminder_date
        .format("%Y-%m-%d %H:%M UTC")
        .to_string();
    let reminder_type = reminder.reminder_type.clone();
    let channel = reminder.channel_type.clone();
    let status = reminder.status.clone();
    let source = reminder.reminder_source.clone();
    let can_cancel = reminder.status == "pending";

    view! {
        <tr>
            <td class="mono text-sm">{date}</td>
            <td class="text-sm">{humanize_reminder_type(&reminder_type)}</td>
            <td class="text-sm">{channel}</td>
            <td><StatusBadge status=status.clone() /></td>
            <td class="text-xs subtle">{source}</td>
            <td>
                <Show when=move || can_cancel fallback=|| view! { <span class="subtle">"—"</span> }>
                    <button
                        class="btn btn-ghost btn-sm"
                        type="button"
                        on:click=move |_| on_cancel_request.run(id)
                    >
                        "Cancel"
                    </button>
                </Show>
            </td>
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
