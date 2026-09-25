//! Route guard: `<ProtectedRoute>`.
//!
//! Renders its children only when the user is authenticated. Otherwise
//! it redirects to `/login`.
//!
//! The guard waits for `bootstrap_done` before deciding. This avoids a
//! brief logout on every page refresh while `GET /api/auth/me` is still
//! in flight.
//!
//! **This is a UX guard, not a security boundary.** The backend
//! independently rejects any request without a valid session.

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

use crate::auth::context::use_auth;

#[component]
pub fn ProtectedRoute(children: ChildrenFn) -> impl IntoView {
    let auth = use_auth();
    let navigate = use_navigate();

    let is_authed = Signal::derive(move || auth.user.get().is_some());
    let bootstrap_done = Signal::derive(move || auth.bootstrap_done.get());

    // `StoredValue` lets the children closure be called from multiple
    // places without moving it (children must implement `Fn`, not
    // `FnOnce`).
    let children = StoredValue::new(children);

    Effect::new({
        let navigate = navigate.clone();
        move |_| {
            // Only redirect once bootstrap has finished AND we know
            // there is no user.
            if bootstrap_done.get() && !is_authed.get() {
                navigate("/login", Default::default());
            }
        }
    });

    view! {
        <Show
            when=move || bootstrap_done.get() && is_authed.get()
            fallback=|| ()
        >
            {children.with_value(|c| c())}
        </Show>
    }
}
