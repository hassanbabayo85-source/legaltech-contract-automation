//! `/notification-channels` — manage delivery channels.
//!
//! Rendered as a list of cards (one per channel type). The backend
//! supports Telegram, Discord, and generic webhook. Email and SMS are
//! shown as "coming soon" placeholders so the layout matches the
//! design language, and are clearly disabled.

use leptos::prelude::*;
use uuid::Uuid;

use crate::api::models::ChannelResponse;
use crate::api::ApiError;
use crate::auth::context::use_auth;
use crate::components::icons::{IconChannels, IconGlobe, IconMail, IconSend, IconTrash};
use crate::components::{AppShell, ConfirmDialog, EmptyState, ErrorState, ListSkeleton, Spinner};
use crate::state::use_toasts;

pub mod edit_channel_form;
pub mod new_channel_form;

pub use edit_channel_form::EditChannelForm;
pub use new_channel_form::NewChannelForm;

const PAGE_SIZE: u32 = 50;

#[component]
pub fn ChannelsPage() -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();

    let channels = RwSignal::new(None::<Result<Vec<ChannelResponse>, ApiError>>);
    let reload = RwSignal::new(0u32);
    let creating = RwSignal::new(false);
    let editing = RwSignal::new(None::<ChannelResponse>);
    let pending_delete = RwSignal::new(None::<Uuid>);

    Effect::new(move |_| {
        reload.get();
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            match client.list_channels(1, PAGE_SIZE).await {
                Ok(page) => channels.set(Some(Ok(page.items))),
                Err(err) if err.is_unauthorized() => {
                    auth.clear();
                    toasts.error("Your session has expired.");
                    channels.set(Some(Err(err)));
                }
                Err(err) => channels.set(Some(Err(err))),
            }
        });
    });

    let on_retry = Callback::new(move |_: ()| reload.update(|v| *v += 1));
    let on_new = Callback::new(move |_: ()| creating.set(true));
    let on_cancel_new = Callback::new(move |_: ()| creating.set(false));
    let on_saved = Callback::new(move |_: ()| {
        creating.set(false);
        editing.set(None);
        reload.update(|v| *v += 1);
    });
    let on_edit = Callback::new(move |c: ChannelResponse| editing.set(Some(c)));
    let on_cancel_edit = Callback::new(move |_: ()| editing.set(None));
    let on_delete_request = Callback::new(move |id: Uuid| pending_delete.set(Some(id)));
    let on_cancel_delete_channel = Callback::new(move |_: ()| pending_delete.set(None));

    let on_delete_confirm = Callback::new(move |_: ()| {
        let Some(id) = pending_delete.get_untracked() else {
            return;
        };
        let auth = auth;
        let toasts = toasts;
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            match client.delete_channel(id).await {
                Ok(()) => {
                    toasts.success("Channel deleted.");
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
                    <h1 class="page-title">"Notification Channels"</h1>
                    <p class="page-desc">"Configure how you receive reminders and alerts."</p>
                </div>
                <div class="page-actions">
                    <button
                        class="btn btn-primary"
                        type="button"
                        on:click=move |_| on_new.run(())
                    >
                        "Add channel"
                    </button>
                </div>
            </div>

            <ConfirmDialog
                open=Signal::derive(move || pending_delete.get().is_some())
                title="Delete channel?"
                message="This removes the channel and its encrypted credentials. Reminders already scheduled on this channel will fail."
                confirm_label="Delete"
                destructive=true
                on_cancel=on_cancel_delete_channel
                on_confirm=on_delete_confirm
            />

            <Show when=move || creating.get()>
                <NewChannelForm on_created=on_saved on_cancel=on_cancel_new />
            </Show>

            <Show when=move || editing.get().is_some()>
                {move || editing.get().map(|c| view! {
                    <EditChannelForm channel=c on_saved=on_saved on_cancel=on_cancel_edit />
                })}
            </Show>

            {move || match channels.get() {
                None => view! { <ListSkeleton rows=3 /> }.into_any(),
                Some(Err(err)) => view! {
                    <ErrorState message=err.user_message() on_retry=on_retry />
                }.into_any(),
                Some(Ok(items)) => {
                    let has_any = !items.is_empty();
                    view! {
                        {if !has_any {
                            view! {
                                <div class="card">
                                    <EmptyState
                                        title="No channels configured"
                                        description="Add a Telegram, Discord, or webhook channel to start receiving reminders."
                                        icon=ViewFn::from(|| view! { <IconChannels /> }.into_any())
                                    />
                                </div>
                            }.into_any()
                        } else {
                            view! { <></> }.into_any()
                        }}

                        <div class="channel-list">
                            {items.into_iter().map(|c| view! {
                                <ChannelCard
                                    channel=c
                                    on_edit=on_edit
                                    on_delete=on_delete_request
                                />
                            }).collect_view()}

                            <ComingSoonCard
                                title="Email"
                                description="Receive notifications via email."
                                icon_kind="mail"
                            />
                            <ComingSoonCard
                                title="SMS"
                                description="Get text message notifications."
                                icon_kind="globe"
                            />
                        </div>
                    }.into_any()
                }
            }}
        </AppShell>
    }
}

#[component]
fn ChannelCard(
    channel: ChannelResponse,
    on_edit: Callback<ChannelResponse>,
    on_delete: Callback<Uuid>,
) -> impl IntoView {
    let name = channel.name.clone();
    let kind = channel.channel_type.clone();
    let enabled = channel.enabled;
    let id = channel.id;
    let kind_label = kind_label(&kind);
    let kind_class = kind_class(&kind);
    let status_label = if enabled { "Connected" } else { "Disabled" };
    let status_class = if enabled {
        "status-connected"
    } else {
        "status-neutral"
    };

    let channel_for_edit = channel.clone();
    let on_edit_click = move |_| on_edit.run(channel_for_edit.clone());
    let on_delete_click = move |_| on_delete.run(id);

    view! {
        <div class="channel-card">
            <div class="channel-icon">
                <span class=kind_class aria-hidden="true">
                    <IconChannels />
                </span>
            </div>
            <div class="channel-body">
                <div class="row row-between" style="margin-bottom: 2px;">
                    <div class="channel-title">{name}</div>
                    <span class=format!("badge {status_class}")>{status_label}</span>
                </div>
                <div class="channel-desc">
                    {kind_label}
                </div>
            </div>
            <div class="row" style="gap: var(--s-1); flex: none;">
                <TestChannelButton channel_id=id />
                <button
                    class="icon-btn"
                    type="button"
                    aria-label="Edit channel"
                    title="Edit"
                    on:click=on_edit_click
                >
                    "Edit"
                </button>
                <button
                    class="icon-btn icon-btn-danger"
                    type="button"
                    aria-label="Delete channel"
                    title="Delete"
                    on:click=on_delete_click
                >
                    <IconTrash />
                </button>
            </div>
        </div>
    }
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "telegram" => "Telegram bot",
        "discord" => "Discord webhook",
        "webhook" => "Generic webhook",
        _ => "Channel",
    }
}

fn kind_class(_kind: &str) -> &'static str {
    "channel-icon-inner"
}

#[component]
fn TestChannelButton(channel_id: Uuid) -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();
    let sending = RwSignal::new(false);

    let on_test = move |_| {
        if sending.get_untracked() {
            return;
        }
        sending.set(true);
        let auth = auth;
        let toasts = toasts;
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            match client.test_channel(channel_id).await {
                Ok(_) => {
                    toasts.success("Test delivered.");
                    sending.set(false);
                }
                Err(err) if err.is_unauthorized() => {
                    auth.clear();
                    toasts.error("Your session has expired.");
                    sending.set(false);
                }
                Err(err) => {
                    toasts.warning(format!("Test failed: {}", err.user_message()));
                    sending.set(false);
                }
            }
        });
    };

    view! {
        <button
            class="icon-btn"
            type="button"
            aria-label="Send test"
            title="Send test"
            disabled=move || sending.get()
            on:click=on_test
        >
            <Show when=move || sending.get() fallback=|| view! { <IconSend /> }>
                <Spinner />
            </Show>
        </button>
    }
}

#[component]
fn ComingSoonCard(
    #[prop(into)] title: String,
    #[prop(into)] description: String,
    #[prop(into)] icon_kind: String,
) -> impl IntoView {
    let icon_view = match icon_kind.as_str() {
        "mail" => view! { <IconMail /> }.into_any(),
        _ => view! { <IconGlobe /> }.into_any(),
    };

    view! {
        <div class="channel-card" style="opacity: 0.55;">
            <div class="channel-icon">{icon_view}</div>
            <div class="channel-body">
                <div class="row row-between" style="margin-bottom: 2px;">
                    <div class="channel-title">{title}</div>
                    <span class="badge status-neutral">"Not available"</span>
                </div>
                <div class="channel-desc">{description}</div>
            </div>
            <button class="btn btn-ghost btn-sm" disabled=true title="Coming soon">
                "Configure"
            </button>
        </div>
    }
}
