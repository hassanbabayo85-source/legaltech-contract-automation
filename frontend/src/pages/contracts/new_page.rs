//! `/contracts/new` — create a contract, with optional PDF/image upload
//! and optional automatic AI analysis.

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use wasm_bindgen::JsCast;

use crate::api::models::CreateContractRequest;
use crate::auth::context::use_auth;
use crate::components::icons::{IconBolt, IconFile, IconGlobe};
use crate::components::{AppShell, Spinner};
use crate::state::use_toasts;

const MAX_TITLE_LEN: usize = 500;
const MAX_RAW_TEXT_LEN: usize = 1024 * 1024; // 1 MiB
const MIN_RAW_TEXT_LEN: usize = 1;
/// Must match the backend's `EXTRACT_BODY_LIMIT_BYTES`.
const MAX_UPLOAD_BYTES: f64 = 20.0 * 1024.0 * 1024.0;

#[component]
pub fn ContractNewPage() -> impl IntoView {
    let auth = use_auth();
    let toasts = use_toasts();
    let navigate = use_navigate();

    let title = RwSignal::new(String::new());
    let raw_text = RwSignal::new(String::new());
    let auto_analyze = RwSignal::new(true);
    let submitting = RwSignal::new(false);
    let uploading = RwSignal::new(false);
    let upload_name = RwSignal::new(None::<String>);
    let error = RwSignal::new(None::<String>);

    let pdf_input_ref = NodeRef::<leptos::html::Input>::new();
    let image_input_ref = NodeRef::<leptos::html::Input>::new();

    // ---- PDF upload ---------------------------------------------------------

    let on_pdf_change = {
        move |ev: web_sys::Event| {
            let Some(input) = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            else {
                return;
            };
            let Some(files) = input.files() else { return };
            if files.length() == 0 {
                return;
            }
            let Some(file) = files.get(0) else { return };

            if file.size() > MAX_UPLOAD_BYTES {
                error.set(Some("PDF is too large (max 20 MiB).".to_string()));
                return;
            }

            let name = file.name();
            error.set(None);
            uploading.set(true);
            upload_name.set(Some(name));

            leptos::task::spawn_local(async move {
                let client = auth.client.get();
                match client.extract_text_from_pdf(file).await {
                    Ok(text) => {
                        raw_text.set(text);
                        toasts.success("PDF text extracted. Review and edit before creating.");
                    }
                    Err(err) => {
                        error.set(Some(err.user_message()));
                        upload_name.set(None);
                    }
                }
                uploading.set(false);
            });
        }
    };

    let on_pdf_click = move |_| {
        if let Some(input) = pdf_input_ref.get_untracked() {
            input.click();
            input.set_value("");
        }
    };

    // ---- Image upload -------------------------------------------------------

    let on_image_change = {
        move |ev: web_sys::Event| {
            let Some(input) = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            else {
                return;
            };
            let Some(files) = input.files() else { return };
            if files.length() == 0 {
                return;
            }
            let Some(file) = files.get(0) else { return };

            if file.size() > MAX_UPLOAD_BYTES {
                error.set(Some("Image is too large (max 20 MiB).".to_string()));
                return;
            }

            let name = file.name();
            error.set(None);
            uploading.set(true);
            upload_name.set(Some(name));

            leptos::task::spawn_local(async move {
                let client = auth.client.get();
                match client.extract_text_from_image(file).await {
                    Ok(text) => {
                        raw_text.set(text);
                        toasts.success("Image text extracted. Review and edit before creating.");
                    }
                    Err(err) => {
                        error.set(Some(err.user_message()));
                        upload_name.set(None);
                    }
                }
                uploading.set(false);
            });
        }
    };

    let on_image_click = move |_| {
        if let Some(input) = image_input_ref.get_untracked() {
            input.click();
            input.set_value("");
        }
    };

    // ---- Submit -------------------------------------------------------------

    let on_submit = {
        let navigate = navigate.clone();
        move |ev: web_sys::SubmitEvent| {
            ev.prevent_default();
            if submitting.get_untracked() {
                return;
            }

            let title_val = title.get_untracked().trim().to_string();
            let raw_text_val = raw_text.get_untracked();
            let analyze_after = auto_analyze.get_untracked();

            if title_val.is_empty() {
                error.set(Some("Title is required.".into()));
                return;
            }
            if title_val.len() > MAX_TITLE_LEN {
                error.set(Some(format!(
                    "Title must be at most {MAX_TITLE_LEN} characters."
                )));
                return;
            }
            if raw_text_val.trim().len() < MIN_RAW_TEXT_LEN {
                error.set(Some("Contract text is required.".into()));
                return;
            }
            if raw_text_val.len() > MAX_RAW_TEXT_LEN {
                error.set(Some("Contract text is too large (max 1 MiB).".into()));
                return;
            }

            error.set(None);
            submitting.set(true);

            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let client = auth.client.get();
                let req = CreateContractRequest {
                    title: title_val,
                    raw_text: raw_text_val,
                };
                let created = match client.create_contract(&req).await {
                    Ok(c) => c,
                    Err(err) => {
                        error.set(Some(err.user_message()));
                        submitting.set(false);
                        return;
                    }
                };
                let id = created.id;

                if analyze_after {
                    match client.analyze_contract(id).await {
                        Ok(_) => toasts.success("Contract created and analyzed."),
                        Err(err) => toasts.warning(format!(
                            "Contract saved, but analysis failed: {}",
                            err.user_message()
                        )),
                    }
                } else {
                    toasts.success("Contract created.");
                }

                navigate(&format!("/contracts/{id}"), Default::default());
            });
        }
    };

    let char_count = Signal::derive(move || raw_text.get().chars().count());
    let over_limit = Signal::derive(move || raw_text.get().len() > MAX_RAW_TEXT_LEN);

    view! {
        <AppShell>
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Create New Contract"</h1>
                    <p class="page-desc">"Add a new contract and let AI identify risks and extract deadlines."</p>
                </div>
            </div>

            <Show when=move || error.get().is_some()>
                <div class="alert alert-error mb-4" role="alert">
                    {move || error.get().unwrap_or_default()}
                </div>
            </Show>

            <form class="card" on:submit=on_submit novalidate=true>
                <div class="field">
                    <label for="contract-title">"Contract title"</label>
                    <input
                        id="contract-title"
                        class="input"
                        type="text"
                        placeholder="e.g. Service Agreement with Acme Corp"
                        required=true
                        maxlength=MAX_TITLE_LEN.to_string()
                        prop:value=move || title.get()
                        on:input=move |ev| title.set(event_target_value(&ev))
                    />
                    <span class="hint">{format!("Up to {MAX_TITLE_LEN} characters.")}</span>
                </div>

                <div class="field">
                    <label for="contract-text">"Contract content"</label>

                    // Two upload zones side by side. Each has its own
                    // hidden file input; the visible button opens the
                    // picker. Only one upload can be in flight at a time.
                    <div class="upload-row">
                        // PDF zone
                        <div class="upload-zone">
                            <span class="upload-zone-icon" aria-hidden="true">
                                <IconFile />
                            </span>
                            <div class="upload-zone-body">
                                <div class="upload-zone-title">"PDF"</div>
                                <div class=move || {
                                    if uploading.get() && upload_name.get().as_deref().is_some_and(|n| n.to_lowercase().ends_with(".pdf")) {
                                        "upload-zone-hint is-loading"
                                    } else if upload_name.get().as_deref().is_some_and(|n| n.to_lowercase().ends_with(".pdf")) {
                                        "upload-zone-hint is-loaded"
                                    } else {
                                        "upload-zone-hint"
                                    }
                                }>
                                    {move || {
                                        if uploading.get() && upload_name.get().as_deref().is_some_and(|n| n.to_lowercase().ends_with(".pdf")) {
                                            "Extracting text…".to_string()
                                        } else if let Some(n) = upload_name.get() {
                                            if n.to_lowercase().ends_with(".pdf") {
                                                let short = if n.chars().count() > 22 {
                                                    let s: String = n.chars().take(19).collect();
                                                    format!("{s}…")
                                                } else {
                                                    n
                                                };
                                                format!("Loaded: {short}")
                                            } else {
                                                "Choose a PDF file (max 20 MiB)".to_string()
                                            }
                                        } else {
                                            "Choose a PDF file (max 20 MiB)".to_string()
                                        }
                                    }}
                                </div>
                            </div>
                            <input
                                node_ref=pdf_input_ref
                                type="file"
                                accept=".pdf,application/pdf"
                                style="display: none;"
                                on:change=on_pdf_change
                            />
                            <button
                                type="button"
                                class="btn btn-primary btn-sm"
                                disabled=move || uploading.get()
                                on:click=on_pdf_click
                            >
                                "Choose"
                            </button>
                        </div>

                        // Image zone
                        <div class="upload-zone">
                            <span class="upload-zone-icon" aria-hidden="true">
                                <IconGlobe />
                            </span>
                            <div class="upload-zone-body">
                                <div class="upload-zone-title">"Image"</div>
                                <div class=move || {
                                    let is_img = upload_name.get().as_deref().is_some_and(|n| {
                                        let l = n.to_lowercase();
                                        l.ends_with(".jpg") || l.ends_with(".jpeg") || l.ends_with(".png") || l.ends_with(".webp")
                                    });
                                    if uploading.get() && is_img {
                                        "upload-zone-hint is-loading"
                                    } else if is_img {
                                        "upload-zone-hint is-loaded"
                                    } else {
                                        "upload-zone-hint"
                                    }
                                }>
                                    {move || {
                                        let is_img = upload_name.get().as_deref().is_some_and(|n| {
                                            let l = n.to_lowercase();
                                            l.ends_with(".jpg") || l.ends_with(".jpeg") || l.ends_with(".png") || l.ends_with(".webp")
                                        });
                                        if uploading.get() && is_img {
                                            "Reading image…".to_string()
                                        } else if is_img {
                                            let n = upload_name.get().unwrap_or_default();
                                            let short = if n.chars().count() > 22 {
                                                let s: String = n.chars().take(19).collect();
                                                format!("{s}…")
                                            } else {
                                                n
                                            };
                                            format!("Loaded: {short}")
                                        } else {
                                            "JPEG, PNG, or WebP (max 20 MiB)".to_string()
                                        }
                                    }}
                                </div>
                            </div>
                            <input
                                node_ref=image_input_ref
                                type="file"
                                accept="image/jpeg,image/png,image/webp"
                                style="display: none;"
                                on:change=on_image_change
                            />
                            <button
                                type="button"
                                class="btn btn-primary btn-sm"
                                disabled=move || uploading.get()
                                on:click=on_image_click
                            >
                                "Choose"
                            </button>
                        </div>
                    </div>

                    <p class="hint" style="margin-top: var(--s-3);">
                        "Text will be extracted into the box below. Review and edit before creating."
                    </p>

                    <textarea
                        id="contract-text"
                        class="input"
                        rows="18"
                        placeholder="Paste the full contract text here, or upload a PDF/image above…"
                        required=true
                        prop:value=move || raw_text.get()
                        on:input=move |ev| raw_text.set(event_target_value(&ev))
                    ></textarea>
                    <span class="hint">
                        {move || format!(
                            "{} characters · whitespace is preserved · max 1 MiB",
                            char_count.get()
                        )}
                    </span>
                    <Show when=move || over_limit.get()>
                        <span class="hint" style="color: var(--c-danger);">
                            "Text exceeds the 1 MiB limit. Please shorten it before creating."
                        </span>
                    </Show>
                </div>

                <div class="card" style="background: var(--c-surface-alt); border-style: dashed; margin-bottom: var(--s-4); box-shadow: none;">
                    <div class="row" style="gap: var(--s-3); align-items: flex-start;">
                        <span class="feature-icon" style="flex: none;">
                            <IconBolt />
                        </span>
                        <div style="flex: 1; min-width: 0;">
                            <div class="card-title" style="margin-bottom: var(--s-1);">"AI analysis"</div>
                            <label class="checkbox-row" style="justify-content: flex-start; margin-bottom: 0;">
                                <input
                                    type="checkbox"
                                    prop:checked=move || auto_analyze.get()
                                    on:change=move |ev| {
                                        let checked = event_target_checked(&ev);
                                        auto_analyze.set(checked);
                                    }
                                />
                                <span>
                                    "Run analysis automatically after creation"
                                </span>
                            </label>
                            <p class="hint mt-2" style="margin-bottom: 0;">
                                "Extracts risks and obligations. You can also run analysis later from the contract page."
                            </p>
                        </div>
                    </div>
                </div>

                <div class="btn-row" style="justify-content: flex-end;">
                    <a class="btn btn-ghost" href="/contracts">"Cancel"</a>
                    <button
                        class="btn btn-primary"
                        type="submit"
                        disabled=move || submitting.get() || uploading.get()
                    >
                        <Show
                            when=move || submitting.get()
                            fallback=|| "Create Contract"
                        >
                            <Spinner />
                            <span>"Creating…"</span>
                        </Show>
                    </button>
                </div>
            </form>

            <Disclaimer />
        </AppShell>
    }
}

#[component]
fn Disclaimer() -> impl IntoView {
    view! {
        <div class="disclaimer mt-4" role="note">
            <strong>"Not legal advice. "</strong>
            "LexGuard provides AI-assisted contract risk analysis and deadline management. \
             It does not provide legal advice and does not replace a qualified legal professional."
        </div>
    }
}
