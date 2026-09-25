//! Root component: context providers, toast host, router.

use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::auth::context::{provide_auth, use_auth};
use crate::auth::guard::ProtectedRoute;
use crate::pages;
use crate::state::provide_toasts;
use crate::state::ToastHost;

#[component]
pub fn App() -> impl IntoView {
    let _auth = provide_auth();
    provide_toasts();
    let _theme = crate::state::theme::provide_theme();

    view! {
        <Router>
            <SessionBootstrap />
            <Routes fallback=|| view! { <NotFound/> }>
                <Route path=path!("/login") view=pages::login::LoginPage />
                <Route path=path!("/register") view=pages::register::RegisterPage />

                <Route
                    path=path!("/")
                    view=move || view! {
                        <ProtectedRoute>
                            <pages::dashboard::DashboardPage />
                        </ProtectedRoute>
                    }
                />
                <Route
                    path=path!("/contracts")
                    view=move || view! {
                        <ProtectedRoute>
                            <pages::contracts::ContractsPage />
                        </ProtectedRoute>
                    }
                />
                <Route
                    path=path!("/contracts/new")
                    view=move || view! {
                        <ProtectedRoute>
                            <pages::contracts::ContractNewPage />
                        </ProtectedRoute>
                    }
                />
                <Route
                    path=path!("/contracts/:id")
                    view=move || view! {
                        <ProtectedRoute>
                            <pages::contracts::ContractDetailPage />
                        </ProtectedRoute>
                    }
                />
                <Route
                    path=path!("/reminders")
                    view=move || view! {
                        <ProtectedRoute>
                            <pages::reminders::RemindersPage />
                        </ProtectedRoute>
                    }
                />
                <Route
                    path=path!("/notification-channels")
                    view=move || view! {
                        <ProtectedRoute>
                            <pages::channels::ChannelsPage />
                        </ProtectedRoute>
                    }
                />
                <Route
                    path=path!("/settings")
                    view=move || view! {
                        <ProtectedRoute>
                            <pages::settings::SettingsPage />
                        </ProtectedRoute>
                    }
                />
            </Routes>
            <ToastHost />
        </Router>
    }
}

/// Attempts to restore the session from a stored token on app start.
///
/// Runs once. Calls `GET /api/auth/me`; on success installs the user in
/// the auth context, on 401 clears the token.
#[component]
fn SessionBootstrap() -> impl IntoView {
    let auth = use_auth();

    Effect::new(move |_| {
        let auth = auth;
        // Only run once.
        if auth.user.get_untracked().is_some() {
            auth.bootstrap_done.set(true);
            return;
        }
        let has_token = auth.client.get_untracked().token().is_some();
        if !has_token {
            // No token — nothing to bootstrap.
            auth.bootstrap_done.set(true);
            return;
        }

        leptos::task::spawn_local(async move {
            let client = auth.client.get();
            match client.me().await {
                Ok(user) => {
                    auth.user.set(Some(user.into()));
                }
                Err(err) if err.is_unauthorized() => {
                    auth.clear();
                }
                Err(err) => {
                    // Network or server error — keep the token, do not
                    // log the user out. The user can retry by navigating.
                    tracing::warn!(
                        category = err.category(),
                        "session bootstrap failed (non-401); keeping token"
                    );
                }
            }
            // Whether success, 401, or network error, bootstrap is now
            // done. The guard will make its decision with full info.
            auth.bootstrap_done.set(true);
        });
    });

    view! { <></> }
}

#[component]
fn NotFound() -> impl IntoView {
    view! {
        <div class="auth-page">
            <div class="auth-card">
                <h1 class="auth-title">"Page not found"</h1>
                <p class="auth-sub">"The page you requested does not exist."</p>
                <a class="btn btn-primary" href="/">"Back to dashboard"</a>
            </div>
        </div>
    }
}
