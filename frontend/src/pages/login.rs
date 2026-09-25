//! `/login` — split-layout sign-in: branding panel on the left,
//! form panel on the right. Falls back to a single column below 900px
//! (see `styles/main.css`).

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

use crate::api::models::LoginRequest;
use crate::auth::context::use_auth;
use crate::components::icons::{
    IconBolt, IconChannels, IconClock, IconEye, IconEyeOff, IconGithub, IconGoogle, IconShield,
};
use crate::components::Spinner;
use crate::state::use_toasts;

#[component]
pub fn LoginPage() -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();
    let navigate = use_navigate();

    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let show_password = RwSignal::new(false);

    let on_submit = {
        let navigate = navigate.clone();
        move |ev: web_sys::SubmitEvent| {
            ev.prevent_default();
            if submitting.get_untracked() {
                return;
            }

            let email_val = email.get_untracked().trim().to_string();
            let password_val = password.get_untracked();

            if email_val.is_empty() {
                error.set(Some("Email is required.".into()));
                return;
            }
            if password_val.is_empty() {
                error.set(Some("Password is required.".into()));
                return;
            }

            error.set(None);
            submitting.set(true);

            let auth = auth;
            let toasts = toasts;
            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let client = auth.client.get();
                let req = LoginRequest {
                    email: email_val,
                    password: password_val,
                };
                match client.login(&req).await {
                    Ok(resp) => {
                        let user = resp.user.into();
                        auth.install_session(user, resp.token);
                        toasts.success("Signed in.");
                        navigate("/", Default::default());
                    }
                    Err(err) => {
                        error.set(Some(err.user_message()));
                        submitting.set(false);
                    }
                }
            });
        }
    };

    view! {
        <div class="auth-page">
            <aside class="auth-brand-panel">
                <div class="auth-brand-logo">"LexGuard"</div>
                <div>
                    <h2 class="auth-brand-headline">
                        "Smart Contract Management for a Safer Tomorrow"
                    </h2>
                    <p class="auth-brand-sub">
                        "AI-powered contract analysis, automated reminders and secure notifications."
                    </p>
                    <ul class="auth-brand-features">
                        <li><span class="icon"><IconBolt /></span>"Analyze contracts with AI"</li>
                        <li><span class="icon"><IconClock /></span>"Get deadline reminders"</li>
                        <li><span class="icon"><IconChannels /></span>"Multiple notification channels"</li>
                        <li><span class="icon"><IconShield /></span>"Enterprise-grade security"</li>
                    </ul>
                </div>
                <div class="text-xs" style="opacity: 0.65;">
                    "AI-assisted contract risk analysis — not legal advice."
                </div>
            </aside>

            <section class="auth-form-panel">
                <form class="auth-card" on:submit=on_submit novalidate=true>
                    <h1 class="auth-title">"Welcome Back"</h1>
                    <p class="auth-sub">"Sign in to your LexGuard account"</p>

                    <Show when=move || error.get().is_some()>
                        <div class="alert alert-error mb-3" role="alert">
                            {move || error.get().unwrap_or_default()}
                        </div>
                    </Show>

                    <div class="field">
                        <label for="login-email">"Email address"</label>
                        <input
                            id="login-email"
                            class="input"
                            type="email"
                            placeholder="you@example.com"
                            autocomplete="username"
                            required=true
                            prop:value=move || email.get()
                            on:input=move |ev| email.set(event_target_value(&ev))
                        />
                    </div>

                    <div class="field">
                        <label for="login-password">"Password"</label>
                        <div class="password-wrap">
                            <input
                                id="login-password"
                                class="input"
                                type=move || if show_password.get() { "text" } else { "password" }
                                placeholder="Enter your password"
                                autocomplete="current-password"
                                required=true
                                prop:value=move || password.get()
                                on:input=move |ev| password.set(event_target_value(&ev))
                            />
                            <button
                                type="button"
                                class="password-toggle"
                                aria-label=move || if show_password.get() { "Hide password" } else { "Show password" }
                                on:click=move |_| show_password.update(|v| *v = !*v)
                            >
                                {move || if show_password.get() {
                                    view! { <IconEyeOff /> }.into_any()
                                } else {
                                    view! { <IconEye /> }.into_any()
                                }}
                            </button>
                        </div>
                    </div>

                    <div class="checkbox-row">
                        <label>
                            <input type="checkbox" />
                            "Remember me"
                        </label>
                        <span
                            class="text-sm muted"
                            style="cursor: not-allowed;"
                            title="Password reset is not yet available"
                        >
                            "Forgot password? (coming soon)"
                        </span>
                    </div>

                    <button
                        class="btn btn-primary btn-block"
                        type="submit"
                        disabled=move || submitting.get()
                    >
                        <Show when=move || submitting.get() fallback=|| "Sign in">
                            <Spinner />
                            <span>"Signing in…"</span>
                        </Show>
                    </button>

                    <div class="social-divider"><span>"or continue with"</span></div>
                    <div class="social-row">
                        <button
                            class="btn btn-ghost"
                            type="button"
                            disabled=true
                            title="OAuth is not yet available"
                        >
                            <IconGoogle />
                            <span>"Google"</span>
                        </button>
                        <button
                            class="btn btn-ghost"
                            type="button"
                            disabled=true
                            title="OAuth is not yet available"
                        >
                            <IconGithub />
                            <span>"GitHub"</span>
                        </button>
                    </div>

                    <p class="auth-alt">
                        "Don't have an account? "
                        <a href="/register">"Create one"</a>
                    </p>
                </form>
            </section>
        </div>
    }
}
