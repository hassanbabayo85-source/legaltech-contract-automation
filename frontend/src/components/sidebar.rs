//! `<Sidebar>` — primary navigation with inline SVG icons.
//!
//! Active state comes from `leptos_router`'s current location, so it
//! updates without a full page reload.

use leptos::prelude::*;
use leptos_router::hooks::use_location;

use crate::components::icons::{
    IconChannels, IconContracts, IconDashboard, IconReminders, IconSettings,
};

#[component]
pub fn Sidebar() -> impl IntoView {
    let location = use_location();

    let is_active = move |path: &str| -> bool {
        let current = location.pathname.get();
        if path == "/" {
            current == "/"
        } else {
            current == path || current.starts_with(&format!("{path}/"))
        }
    };

    view! {
        <nav class="nav" aria-label="Primary">
            <a
                href="/"
                class=move || if is_active("/") { "active" } else { "" }
                aria-current=move || if is_active("/") { "page" } else { "false" }
            >
                <IconDashboard />
                <span>"Dashboard"</span>
            </a>
            <a
                href="/contracts"
                class=move || if is_active("/contracts") { "active" } else { "" }
                aria-current=move || if is_active("/contracts") { "page" } else { "false" }
            >
                <IconContracts />
                <span>"Contracts"</span>
            </a>
            <a
                href="/reminders"
                class=move || if is_active("/reminders") { "active" } else { "" }
                aria-current=move || if is_active("/reminders") { "page" } else { "false" }
            >
                <IconReminders />
                <span>"Reminders"</span>
            </a>
            <a
                href="/notification-channels"
                class=move || if is_active("/notification-channels") { "active" } else { "" }
                aria-current=move || if is_active("/notification-channels") { "page" } else { "false" }
            >
                <IconChannels />
                <span>"Channels"</span>
            </a>
            <div class="nav-group-label">"Account"</div>
            <a
                href="/settings"
                class=move || if is_active("/settings") { "active" } else { "" }
                aria-current=move || if is_active("/settings") { "page" } else { "false" }
            >
                <IconSettings />
                <span>"Settings"</span>
            </a>
        </nav>
    }
}
