//! `<SafeText>` — renders a string as text, never as HTML.
//!
//! This is the *only* way contract text and AI output should reach the
//! DOM. Leptos' `view!` macro inserts strings as text nodes by default,
//! but this component makes the safety property explicit and searchable.
//!
//! Do NOT use `inner_html` on untrusted content. If a future feature
//! genuinely needs rich formatting, use a well-known sanitizer and
//! document why. See `docs/FRONTEND_SECURITY.md`.

use leptos::prelude::*;

#[component]
pub fn SafeText(
    #[prop(into)] text: String,
    /// Optional extra CSS class for the wrapper `<span>`.
    #[prop(optional, into)]
    class: Option<String>,
    /// Render with `white-space: pre-wrap` (preserves newlines) and
    /// word-break. Useful for contract text and AI evidence.
    #[prop(optional)]
    pre_wrap: bool,
) -> impl IntoView {
    let mut cls = String::new();
    if pre_wrap {
        cls.push_str("pre-wrap ");
    }
    if let Some(extra) = class {
        cls.push_str(&extra);
    }
    let cls = cls.trim().to_string();
    view! { <span class=cls>{text}</span> }
}
