//! `/register` — split-layout account creation.

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

use crate::api::models::RegisterRequest;
use crate::auth::context::use_auth;
use crate::components::icons::{IconBolt, IconChannels, IconClock, IconEye, IconEyeOff, IconShield};
use crate::components::Spinner;
use crate::state::use_toasts;

/// Minimum password length enforced server-side.
const MIN_PASSWORD_LEN: usize = 12;
const MAX_EMAIL_LEN: usize = 254;
const MAX_FULL_NAME_LEN: usize = 200;

#[component]
pub fn RegisterPage() -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();
    let navigate = use_navigate();

    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let confirm = RwSignal::new(String::new());
    let full_name = RwSignal::new(String::new());
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
            let confirm_val = confirm.get_untracked();
            let full_name_val = full_name.get_untracked().trim().to_string();

            if email_val.is_empty() || !email_val.contains('@') {
                error.set(Some("Please enter a valid email address.".into()));
                return;
            }
            if email_val.len() > MAX_EMAIL_LEN {
                error.set(Some("Email is too long.".into()));
                return;
            }
            if full_name_val.is_empty() {
                error.set(Some("Full name is required.".into()));
                return;
            }
            if full_name_val.len() > MAX_FULL_NAME_LEN {
                error.set(Some("Full name is too long.".into()));
                return;
            }
            if password_val.chars().count() < MIN_PASSWORD_LEN {
                error.set(Some(format!(
                    "Password must be at least {MIN_PASSWORD_LEN} characters."
                )));
                return;
            }
            if password_val != confirm_val {
                error.set(Some("Passwords do not match.".into()));
                return;
            }

            error.set(None);
            submitting.set(true);

            let auth = auth;
            let toasts = toasts;
            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let client = auth.client.get();
                let req = RegisterRequest {
                    email: email_val,
                    password: password_val,
                    full_name: full_name_val,
                };
                match client.register(&req).await {
                    Ok(resp) => {
                        let user = resp.user.into();
                        auth.install_session(user, resp.token);
                        toasts.success("Account created.");
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
                    <h2 class="auth-brand-headline">"Create Your Account"</h2>
                    <p class="auth-brand-sub">
                        "Join LexGuard and start managing your contracts smarter."
                    </p>
                    <ul class="auth-brand-features">
                        <li><span class="icon"><IconBolt /></span>"AI-powered contract analysis"</li>
                        <li><span class="icon"><IconClock /></span>"Automated deadline reminders"</li>
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
                    <h1 class="auth-title">"Create Your Account"</h1>
                    <p class="auth-sub">"Fill in your information to get started."</p>

                    <Show when=move || error.get().is_some()>
                        <div class="alert alert-error mb-3" role="alert">
                            {move || error.get().unwrap_or_default()}
                        </div>
                    </Show>

                    <div class="field">
                        <label for="register-full-name">"Full name"</label>
                        <input
                            id="register-full-name"
                            class="input"
                            type="text"
                            placeholder="John Doe"
                            autocomplete="name"
                            required=true
                            prop:value=move || full_name.get()
                            on:input=move |ev| full_name.set(event_target_value(&ev))
                        />
                    </div>

                    <div class="field">
                        <label for="register-email">"Email address"</label>
                        <input
                            id="register-email"
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
                        <label for="register-password">"Password"</label>
                        <div class="password-wrap">
                            <input
                                id="register-password"
                                class="input"
                                type=move || if show_password.get() { "text" } else { "password" }
                                placeholder="Create a strong password"
                                autocomplete="new-password"
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
                        <span class="hint">
                            {format!("At least {MIN_PASSWORD_LEN} characters. Do not reuse a password from another site.")}
                        </span>
                    </div>

                    <div class="field">
                        <label for="register-confirm">"Confirm password"</label>
                        <div class="password-wrap">
                            <input
                                id="register-confirm"
                                class="input"
                                type=move || if show_password.get() { "text" } else { "password" }
                                placeholder="Confirm your password"
                                autocomplete="new-password"
                                required=true
                                prop:value=move || confirm.get()
                                on:input=move |ev| confirm.set(event_target_value(&ev))
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

                    <button
                        class="btn btn-primary btn-block"
                        type="submit"
                        disabled=move || submitting.get()
                    >
                        <Show when=move || submitting.get() fallback=|| "Create Account">
                            <Spinner />
                            <span>"Creating account…"</span>
                        </Show>
                    </button>

                    <p class="auth-alt">
                        "Already have an account? "
                        <a href="/login">"Sign in"</a>
                    </p>
                </form>
            </section>
        </div>
    }
}
