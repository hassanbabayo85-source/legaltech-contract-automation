//! Toast notifications.
//!
//! A `Toasts` context holds a reactive list of transient messages.
//! Components push messages via the `success` / `error` / `warning` /
//! `info` methods. A single `<ToastHost/>` at the root of the tree
//! renders them.
//!
//! Messages are never HTML. They are always rendered as text (see
//! `components::SafeText`).

use leptos::prelude::*;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub id: Uuid,
    pub kind: ToastKind,
    pub message: String,
}

#[derive(Clone, Copy)]
pub struct Toasts {
    items: RwSignal<Vec<Toast>>,
}

impl Toasts {
    pub fn new() -> Self {
        Self {
            items: RwSignal::new(Vec::new()),
        }
    }

    pub fn push(&self, kind: ToastKind, message: impl Into<String>) {
        let toast = Toast {
            id: Uuid::new_v4(),
            kind,
            message: message.into(),
        };
        let id = toast.id;
        self.items.update(|list| list.push(toast));

        // Auto-dismiss after 5 s.
        let items = self.items;
        leptos::task::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(5_000).await;
            items.update(|list| list.retain(|t| t.id != id));
        });
    }

    pub fn success(&self, msg: impl Into<String>) {
        self.push(ToastKind::Success, msg);
    }
    pub fn error(&self, msg: impl Into<String>) {
        self.push(ToastKind::Error, msg);
    }
    pub fn warning(&self, msg: impl Into<String>) {
        self.push(ToastKind::Warning, msg);
    }
    pub fn info(&self, msg: impl Into<String>) {
        self.push(ToastKind::Info, msg);
    }

    pub fn dismiss(&self, id: Uuid) {
        self.items.update(|list| list.retain(|t| t.id != id));
    }

    pub fn items(&self) -> RwSignal<Vec<Toast>> {
        self.items
    }
}

impl Default for Toasts {
    fn default() -> Self {
        Self::new()
    }
}

/// Installs the toast context into the current reactive scope.
pub fn provide_toasts() -> Toasts {
    let ctx = Toasts::new();
    provide_context(ctx);
    ctx
}

/// Reads the toast context. Panics if `provide_toasts()` was not called.
pub fn use_toasts() -> Toasts {
    use_context::<Toasts>().expect("Toasts not provided; call provide_toasts() in App")
}

/// Renders the toast stack. Place exactly once, near the app root.
#[component]
pub fn ToastHost() -> impl IntoView {
    let toasts = use_toasts();
    let items = toasts.items();

    view! {
        <div
            class="toast-container"
            role="region"
            aria-label="Notifications"
            aria-live="polite"
        >
            <For
                each=move || items.get()
                key=|t| t.id
                children=move |t| {
                    let kind_class = match t.kind {
                        ToastKind::Success => "toast toast-success",
                        ToastKind::Error => "toast toast-error",
                        ToastKind::Warning => "toast toast-warning",
                        ToastKind::Info => "toast toast-info",
                    };
                    let id = t.id;
                    let toasts = toasts;
                    view! {
                        <div class=kind_class role="status">
                            <div class="toast-body">{t.message}</div>
                            <button
                                class="toast-close"
                                type="button"
                                aria-label="Dismiss notification"
                                on:click=move |_| toasts.dismiss(id)
                            >
                                "×"
                            </button>
                        </div>
                    }
                }
            />
        </div>
    }
}
