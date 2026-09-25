//! Create-channel form.
//!
//! Renders a form whose shape depends on the selected channel type.
//! Secrets are typed here but never echoed after submission — the
//! backend returns only safe metadata.

use leptos::prelude::*;
use serde_json::json;

use crate::auth::context::use_auth;
use crate::components::Spinner;
use crate::state::use_toasts;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Telegram,
    Discord,
    Webhook,
}

#[component]
pub fn NewChannelForm(on_created: Callback<()>, on_cancel: Callback<()>) -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();

    let kind = RwSignal::new(Kind::Telegram);
    let name = RwSignal::new(String::new());

    // Per-channel fields. Kept separate so switching kinds does not mix
    // values.
    let telegram_bot_token = RwSignal::new(String::new());
    let telegram_chat_id = RwSignal::new(String::new());
    let discord_webhook_url = RwSignal::new(String::new());
    let webhook_url = RwSignal::new(String::new());
    let webhook_bearer = RwSignal::new(String::new());
    let use_bearer = RwSignal::new(false);

    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let on_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        if submitting.get_untracked() {
            return;
        }

        let name_val = name.get_untracked().trim().to_string();
        if name_val.is_empty() {
            error.set(Some("Name is required.".into()));
            return;
        }
        if name_val.len() > 100 {
            error.set(Some("Name is too long (max 100 characters).".into()));
            return;
        }

        let k = kind.get_untracked();
        let body = match k {
            Kind::Telegram => {
                let token = telegram_bot_token.get_untracked().trim().to_string();
                let chat = telegram_chat_id.get_untracked().trim().to_string();
                if token.is_empty() || chat.is_empty() {
                    error.set(Some("Bot token and chat id are both required.".into()));
                    return;
                }
                json!({
                    "channel_type": "telegram",
                    "name": name_val,
                    "bot_token": token,
                    "chat_id": chat,
                })
            }
            Kind::Discord => {
                let url = discord_webhook_url.get_untracked().trim().to_string();
                if url.is_empty() {
                    error.set(Some("Discord webhook URL is required.".into()));
                    return;
                }
                json!({
                    "channel_type": "discord",
                    "name": name_val,
                    "webhook_url": url,
                })
            }
            Kind::Webhook => {
                let url = webhook_url.get_untracked().trim().to_string();
                if url.is_empty() {
                    error.set(Some("Webhook URL is required.".into()));
                    return;
                }
                let mut obj = json!({
                    "channel_type": "webhook",
                    "name": name_val,
                    "url": url,
                });
                if use_bearer.get_untracked() {
                    let token = webhook_bearer.get_untracked().trim().to_string();
                    if token.is_empty() {
                        error.set(Some("Bearer token is required if auth is enabled.".into()));
                        return;
                    }
                    obj["auth"] = json!({ "type": "bearer", "token": token });
                }
                obj
            }
        };

        error.set(None);
        submitting.set(true);

        let auth = auth;
        let toasts = toasts;
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            match client.create_channel(&body).await {
                Ok(_) => {
                    toasts.success("Channel created.");
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
                <div class="card-title">"New notification channel"</div>
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
                    <label for="new-channel-kind">"Channel type"</label>
                    <select
                        id="new-channel-kind"
                        class="input"
                        on:change=move |ev| {
                            let v = event_target_value(&ev);
                            kind.set(match v.as_str() {
                                "discord" => Kind::Discord,
                                "webhook" => Kind::Webhook,
                                _ => Kind::Telegram,
                            });
                        }
                    >
                        <option value="telegram" selected=move || kind.get() == Kind::Telegram>
                            "Telegram"
                        </option>
                        <option value="discord" selected=move || kind.get() == Kind::Discord>
                            "Discord"
                        </option>
                        <option value="webhook" selected=move || kind.get() == Kind::Webhook>
                            "Webhook"
                        </option>
                    </select>
                </div>

                <div class="field">
                    <label for="new-channel-name">"Name"</label>
                    <input
                        id="new-channel-name"
                        class="input"
                        type="text"
                        required=true
                        prop:value=move || name.get()
                        on:input=move |ev| name.set(event_target_value(&ev))
                    />
                    <span class="hint">"A label to recognise this channel in lists."</span>
                </div>

                <Show when=move || kind.get() == Kind::Telegram>
                    <div class="field">
                        <label for="tg-token">"Telegram bot token"</label>
                        <input
                            id="tg-token"
                            class="input"
                            type="password"
                            autocomplete="off"
                            prop:value=move || telegram_bot_token.get()
                            on:input=move |ev| telegram_bot_token.set(event_target_value(&ev))
                        />
                        <span class="hint">"Format: `<digits>:<string>`. Stored encrypted."</span>
                    </div>
                    <div class="field">
                        <label for="tg-chat">"Telegram chat id"</label>
                        <input
                            id="tg-chat"
                            class="input"
                            type="text"
                            autocomplete="off"
                            prop:value=move || telegram_chat_id.get()
                            on:input=move |ev| telegram_chat_id.set(event_target_value(&ev))
                        />
                        <span class="hint">"Integer id (may be negative) or `@channelname`."</span>
                    </div>
                </Show>

                <Show when=move || kind.get() == Kind::Discord>
                    <div class="field">
                        <label for="dc-url">"Discord webhook URL"</label>
                        <input
                            id="dc-url"
                            class="input"
                            type="password"
                            autocomplete="off"
                            prop:value=move || discord_webhook_url.get()
                            on:input=move |ev| discord_webhook_url.set(event_target_value(&ev))
                        />
                        <span class="hint">
                            "Must be an `https://discord.com/api/webhooks/...` URL. \
                             Treat it as a credential; it is stored encrypted."
                        </span>
                    </div>
                </Show>

                <Show when=move || kind.get() == Kind::Webhook>
                    <div class="field">
                        <label for="wh-url">"Webhook URL"</label>
                        <input
                            id="wh-url"
                            class="input"
                            type="url"
                            autocomplete="off"
                            prop:value=move || webhook_url.get()
                            on:input=move |ev| webhook_url.set(event_target_value(&ev))
                        />
                        <span class="hint">
                            "Must be HTTPS and resolve to a public IP. \
                             Localhost, private ranges, and cloud metadata are blocked."
                        </span>
                    </div>
                    <div class="field">
                        <label>
                            <input
                                type="checkbox"
                                prop:checked=move || use_bearer.get()
                                on:change=move |ev| use_bearer.set(event_target_checked(&ev))
                            />
                            " Send a bearer token on each request"
                        </label>
                    </div>
                    <Show when=move || use_bearer.get()>
                        <div class="field">
                            <label for="wh-token">"Bearer token"</label>
                            <input
                                id="wh-token"
                                class="input"
                                type="password"
                                autocomplete="off"
                                prop:value=move || webhook_bearer.get()
                                on:input=move |ev| webhook_bearer.set(event_target_value(&ev))
                            />
                            <span class="hint">
                                "Sent as `Authorization: Bearer <token>`. Stored encrypted."
                            </span>
                        </div>
                    </Show>
                </Show>

                <div class="row row-between mt-4">
                    <span class="text-xs subtle">
                        "Secrets are encrypted at rest and never returned by any API response."
                    </span>
                    <button
                        class="btn btn-primary"
                        type="submit"
                        disabled=move || submitting.get()
                    >
                        <Show when=move || submitting.get() fallback=|| "Create channel">
                            <Spinner />
                            <span>"Creating…"</span>
                        </Show>
                    </button>
                </div>
            </form>
        </div>
    }
}
