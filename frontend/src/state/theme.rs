//! Light/dark theme state.
//!
//! The initial theme is applied by an inline script in `index.html`
//! (before WASM loads) so there is no flash of the wrong theme. This
//! module reads the current value from the `<html data-theme>` attribute
//! and persists user choices to `localStorage`.

use leptos::prelude::*;

const STORAGE_KEY: &str = "lexhack.theme";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
    pub fn toggled(self) -> Self {
        match self {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        }
    }
}

/// Reads the theme currently applied to `<html data-theme>`.
///
/// Falls back to `Light` if the attribute is missing or the DOM is
/// unavailable (which should not happen in a browser).
pub fn current() -> Theme {
    let Some(win) = web_sys::window() else {
        return Theme::Light;
    };
    let Some(doc) = win.document() else {
        return Theme::Light;
    };
    let Some(html) = doc.document_element() else {
        return Theme::Light;
    };
    match html.get_attribute("data-theme").as_deref() {
        Some("dark") => Theme::Dark,
        _ => Theme::Light,
    }
}

/// Applies `theme` to the DOM and persists the choice.
pub fn set(theme: Theme) {
    let Some(win) = web_sys::window() else { return };
    let Some(doc) = win.document() else { return };
    let Some(html) = doc.document_element() else {
        return;
    };
    let _ = html.set_attribute("data-theme", theme.as_str());
    if let Ok(Some(storage)) = win.local_storage() {
        let _ = storage.set_item(STORAGE_KEY, theme.as_str());
    }
}

/// Provides the current theme as a reactive signal at the current scope.
/// Returns the `RwSignal` so components can both read and toggle it.
pub fn provide_theme() -> RwSignal<Theme> {
    let signal = RwSignal::new(current());
    provide_context(ThemeSignal(signal));
    signal
}

/// Reads the theme signal from context. Panics if `provide_theme` was
/// not called higher in the tree.
pub fn use_theme() -> RwSignal<Theme> {
    use_context::<ThemeSignal>()
        .map(|s| s.0)
        .expect("ThemeSignal not provided; call provide_theme() in App")
}

#[derive(Clone, Copy)]
struct ThemeSignal(RwSignal<Theme>);

/// Convenience: toggle between light and dark, applying and persisting.
pub fn toggle(signal: RwSignal<Theme>) {
    let next = signal.get_untracked().toggled();
    signal.set(next);
    set(next);
}
