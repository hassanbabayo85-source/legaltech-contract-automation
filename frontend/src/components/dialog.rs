//! `<ConfirmDialog>` — accessible confirmation for destructive actions.
//!
//! Uses the native `<dialog>` element so focus management, escape-to-
//! close, and modal semantics come from the browser rather than from
//! hand-rolled JavaScript.

use leptos::prelude::*;
use web_sys::HtmlDialogElement;

#[component]
pub fn ConfirmDialog(
    /// Controls visibility. The parent is responsible for clearing this
    /// when the user cancels — this component does not mutate the
    /// signal itself.
    open: Signal<bool>,
    #[prop(into)] title: String,
    #[prop(into)] message: String,
    /// Label on the confirm button. Defaults to "Confirm".
    #[prop(optional, into)]
    confirm_label: Option<String>,
    /// Label on the cancel button. Defaults to "Cancel".
    #[prop(optional, into)]
    cancel_label: Option<String>,
    /// Adds a `btn-danger` style to the confirm button when true.
    #[prop(optional)]
    destructive: bool,
    /// Called when the user confirms.
    on_confirm: Callback<()>,
    /// Called when the user cancels (Cancel button, Escape key, or
    /// click on backdrop). The parent must clear its `open` signal.
    on_cancel: Callback<()>,
) -> impl IntoView {
    let confirm_label = confirm_label.unwrap_or_else(|| "Confirm".to_string());
    let cancel_label = cancel_label.unwrap_or_else(|| "Cancel".to_string());
    let confirm_class = if destructive {
        "btn btn-danger"
    } else {
        "btn btn-primary"
    };

    let dialog_ref = NodeRef::<leptos::html::Dialog>::new();

    Effect::new(move |_| {
        if let Some(dialog) = dialog_ref.get() {
            let el: &HtmlDialogElement = &dialog;
            if open.get() {
                let _ = el.show_modal();
            } else {
                el.close();
            }
        }
    });

    // Cancellation delegates to the parent, which owns the `open`
    // signal. Confirmation similarly delegates; the parent closes on
    // success by clearing its own signal.
    let on_cancel_click = move |_| {
        on_cancel.run(());
    };
    let on_confirm_click = move |_| {
        on_confirm.run(());
    };
    let on_native_cancel = move |ev: web_sys::Event| {
        ev.prevent_default();
        on_cancel.run(());
    };

    let title_id = "confirm-dialog-title";
    let desc_id = "confirm-dialog-desc";

    view! {
        <dialog
            node_ref=dialog_ref
            class="dialog"
            aria-labelledby=title_id
            aria-describedby=desc_id
            on:cancel=on_native_cancel
        >
            <div class="dialog-title" id=title_id>{title}</div>
            <div class="dialog-body" id=desc_id>{message}</div>
            <div class="dialog-actions">
                <button class="btn btn-ghost" type="button" on:click=on_cancel_click>
                    {cancel_label}
                </button>
                <button class=confirm_class type="button" on:click=on_confirm_click>
                    {confirm_label}
                </button>
            </div>
        </dialog>
    }
}
