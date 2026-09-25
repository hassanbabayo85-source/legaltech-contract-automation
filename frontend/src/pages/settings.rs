//! `/settings` — account settings with tab navigation.
//!
//! Honest scope:
//!   * **Profile**    — read-only view of the signed-in user.
//!   * **Security**   — current session and sign-out.
//!   * **Notifications** — pointer to the Channels page (channels are
//!     managed there, not here).
//!   * **Appearance** — describes the automatic light/dark mode; there
//!     is no user override yet.
//!
//! The backend does not expose account-mutation or preference
//! endpoints. Tabs that would need them are rendered as read-only
//! with an explanatory note, rather than pretending to save changes.

use leptos::prelude::*;

use crate::auth::context::use_auth;
use crate::components::icons::{IconChannels, IconShield};
use crate::components::AppShell;
use crate::state::use_toasts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Profile,
    Security,
    Notifications,
    Appearance,
}

#[component]
pub fn SettingsPage() -> impl IntoView {
    let tab = RwSignal::new(SettingsTab::Profile);

    view! {
        <AppShell>
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Settings"</h1>
                    <p class="page-desc">"Manage your account and application preferences."</p>
                </div>
            </div>

            <div class="tabs" role="tablist" aria-label="Settings sections">
                <button
                    type="button"
                    role="tab"
                    class=move || tab_class(tab.get(), SettingsTab::Profile)
                    aria-selected=move || (tab.get() == SettingsTab::Profile).to_string()
                    on:click=move |_| tab.set(SettingsTab::Profile)
                >
                    "Profile"
                </button>
                <button
                    type="button"
                    role="tab"
                    class=move || tab_class(tab.get(), SettingsTab::Security)
                    aria-selected=move || (tab.get() == SettingsTab::Security).to_string()
                    on:click=move |_| tab.set(SettingsTab::Security)
                >
                    "Security"
                </button>
                <button
                    type="button"
                    role="tab"
                    class=move || tab_class(tab.get(), SettingsTab::Notifications)
                    aria-selected=move || (tab.get() == SettingsTab::Notifications).to_string()
                    on:click=move |_| tab.set(SettingsTab::Notifications)
                >
                    "Notifications"
                </button>
                <button
                    type="button"
                    role="tab"
                    class=move || tab_class(tab.get(), SettingsTab::Appearance)
                    aria-selected=move || (tab.get() == SettingsTab::Appearance).to_string()
                    on:click=move |_| tab.set(SettingsTab::Appearance)
                >
                    "Appearance"
                </button>
            </div>

            {move || match tab.get() {
                SettingsTab::Profile => view! { <ProfileTab/> }.into_any(),
                SettingsTab::Security => view! { <SecurityTab/> }.into_any(),
                SettingsTab::Notifications => view! { <NotificationsTab/> }.into_any(),
                SettingsTab::Appearance => view! { <AppearanceTab/> }.into_any(),
            }}
        </AppShell>
    }
}

fn tab_class(current: SettingsTab, mine: SettingsTab) -> &'static str {
    if current == mine {
        "tab tab-active"
    } else {
        "tab"
    }
}

// =========================================================================
// Profile
// =========================================================================

#[component]
fn ProfileTab() -> impl IntoView {
    let auth = use_auth();

    view! {
        <div class="card">
            <div class="card-header">
                <div class="card-title">"Profile information"</div>
            </div>

            {move || match auth.user.get() {
                None => view! {
                    <p class="muted">"Not signed in."</p>
                }.into_any(),
                Some(u) => {
                    let initials = initials_of(&u.full_name);
                    view! {
                        <div class="avatar-row">
                            <div class="avatar" aria-hidden="true">{initials}</div>
                            <div>
                                <div class="avatar-name">{u.full_name.clone()}</div>
                                <div class="avatar-email">{u.email.clone()}</div>
                            </div>
                        </div>

                        <dl class="kv-list">
                            <dt>"Full name"</dt>
                            <dd>{u.full_name.clone()}</dd>
                            <dt>"Email"</dt>
                            <dd>{u.email.clone()}</dd>
                            <dt>"User ID"</dt>
                            <dd class="mono text-xs">{u.id.to_string()}</dd>
                        </dl>

                        <p class="hint mt-4">
                            "Account details are read-only. Contact support if you need to change them."
                        </p>
                    }.into_any()
                }
            }}
        </div>
    }
}

fn initials_of(name: &str) -> String {
    let mut initials = String::new();
    for word in name.split_whitespace().take(2) {
        if let Some(ch) = word.chars().next() {
            initials.push(ch.to_ascii_uppercase());
        }
    }
    if initials.is_empty() {
        "?".to_string()
    } else {
        initials
    }
}

// =========================================================================
// Security
// =========================================================================

#[component]
fn SecurityTab() -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();

    let on_logout = move |_| {
        let auth = auth;
        let toasts = toasts;
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            let _ = client.logout().await;
            auth.clear();
            toasts.info("Signed out.");
            if let Some(window) = web_sys::window() {
                let _ = window.location().set_href("/login");
            }
        });
    };

    view! {
        <div class="card">
            <div class="card-header">
                <div class="card-title">
                    <span class="row" style="gap: var(--s-2);">
                        <IconShield />
                        <span>"Current session"</span>
                    </span>
                </div>
            </div>

            <p class="muted text-sm">
                "You are signed in with an opaque bearer token stored in this browser. \
                 Signing out revokes the session on the server."
            </p>

            <dl class="kv-list mt-3">
                <dt>"Authentication"</dt>
                <dd>"Argon2id password + opaque session token"</dd>
                <dt>"Token storage"</dt>
                <dd>"Browser localStorage"</dd>
            </dl>

            <div class="mt-4">
                <button class="btn btn-danger" type="button" on:click=on_logout>
                    "Sign out"
                </button>
            </div>

            <p class="hint mt-4">
                "Password change and session listing are not yet available."
            </p>
        </div>
    }
}

// =========================================================================
// Notifications
// =========================================================================

#[component]
fn NotificationsTab() -> impl IntoView {
    view! {
        <div class="card">
            <div class="card-header">
                <div class="card-title">
                    <span class="row" style="gap: var(--s-2);">
                        <IconChannels />
                        <span>"Notification channels"</span>
                    </span>
                </div>
            </div>

            <p class="muted text-sm">
                "Notification channels (Telegram, Discord, webhook) are managed on \
                 their own page. Reminder delivery respects the channel you enable \
                 for each reminder."
            </p>

            <div class="btn-row mt-3">
                <a class="btn btn-primary" href="/notification-channels">
                    "Manage channels"
                </a>
            </div>

            <p class="hint mt-4">
                "Per-channel quiet hours and per-contract overrides are not yet available."
            </p>
        </div>
    }
}

// =========================================================================
// Appearance
// =========================================================================

#[component]
fn AppearanceTab() -> impl IntoView {
    view! {
        <div class="card">
            <div class="card-header">
                <div class="card-title">"Appearance"</div>
            </div>

            <p class="muted text-sm">
                "LexGuard follows your operating system's light/dark preference \
                 automatically. There is no in-app override yet."
            </p>

            <div class="mt-3">
                <div class="toggle-row">
                    <div>
                        <div class="toggle-label">"Theme"</div>
                        <div class="toggle-hint">"Automatically matches your system setting."</div>
                    </div>
                    <span class="badge status-info">"Automatic"</span>
                </div>
                <div class="toggle-row">
                    <div>
                        <div class="toggle-label">"Reduced motion"</div>
                        <div class="toggle-hint">
                            "Honours your system's \"prefers-reduced-motion\" setting."
                        </div>
                    </div>
                    <span class="badge status-info">"Automatic"</span>
                </div>
            </div>

            <p class="hint mt-4">
                "Manual theme and density controls are not yet available."
            </p>
        </div>

        <div class="disclaimer mt-4" role="note">
            <strong>"Not legal advice. "</strong>
            "LexGuard provides AI-assisted contract risk analysis and deadline management. \
             It does not provide legal advice and does not replace a qualified legal professional."
        </div>
    }
}
