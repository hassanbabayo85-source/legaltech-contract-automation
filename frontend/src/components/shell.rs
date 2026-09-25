//! Application shell: sidebar, header, content grid.
//!
//! Manual theme toggle lives in the sidebar footer, next to sign-out.
//! The inline script in `index.html` applies the persisted theme
//! before WASM loads to avoid a flash.

use leptos::prelude::*;

use crate::auth::context::use_auth;
use crate::components::icons::{IconLogout, IconMoon, IconSettings, IconSun, IconUser};
use crate::components::sidebar::Sidebar;
use crate::state::theme::{self, Theme};
use crate::state::use_toasts;

#[component]
pub fn AppShell(children: Children) -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();
    let theme_signal = theme::use_theme();

    let on_logout = move |_| {
        let auth = auth;
        let toasts = toasts;
        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            let _ = client.logout().await;
            auth.clear();
            toasts.info("Signed out.");
        });
    };

    let on_toggle_theme = move |_| {
        theme::toggle(theme_signal);
    };

    let display_name = Signal::derive(move || {
        auth.user
            .get()
            .map(|u| u.full_name)
            .unwrap_or_else(|| "…".to_string())
    });
    let user_email = Signal::derive(move || auth.user.get().map(|u| u.email).unwrap_or_default());

    let theme_icon = move || {
        if theme_signal.get() == Theme::Dark {
            view! { <IconSun /> }.into_any()
        } else {
            view! { <IconMoon /> }.into_any()
        }
    };
    let theme_label = Signal::derive(move || {
        if theme_signal.get() == Theme::Dark {
            "Light mode"
        } else {
            "Dark mode"
        }
    });

    view! {
        <div class="app-shell">
            <aside class="app-sidebar">
                <div class="brand">
                    <span class="brand-mark" aria-hidden="true"></span>
                    <span class="brand-text">
                        <span class="brand-name">"LexGuard"</span>
                        <span class="brand-sub">"Contract risk & deadlines"</span>
                    </span>
                </div>
                <Sidebar />
                <div class="sidebar-footer">
                    <div class="row" style="padding: 0 var(--s-3); gap: var(--s-2);">
                        <span class="icon icon-lg" aria-hidden="true">
                            <IconUser />
                        </span>
                        <div style="min-width: 0;">
                            <div class="sidebar-user" style="padding: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                                {move || display_name.get()}
                            </div>
                            <div class="text-xs muted" style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                                {move || user_email.get()}
                            </div>
                        </div>
                    </div>

                    <button
                        class="btn btn-ghost btn-sm"
                        type="button"
                        on:click=on_toggle_theme
                        aria-label=theme_label
                        title=theme_label
                    >
                        {theme_icon}
                        <span>{theme_label}</span>
                    </button>

                    <button
                        class="btn btn-ghost btn-sm"
                        type="button"
                        on:click=on_logout
                        aria-label="Sign out"
                    >
                        <IconLogout />
                        <span>"Sign out"</span>
                    </button>
                </div>
            </aside>
            <div>
                <header class="app-header">
                    <div class="row row-between" style="width: 100%;">
                        <span class="text-sm muted">"AI-assisted contract risk analysis — not legal advice."</span>
                        <nav aria-label="Session">
                            <a class="text-sm row" href="/settings" style="gap: var(--s-1);">
                                <IconSettings />
                                <span>"Settings"</span>
                            </a>
                        </nav>
                    </div>
                </header>
                <main class="app-main" id="main-content">
                    {children()}
                </main>
            </div>
        </div>
    }
}
