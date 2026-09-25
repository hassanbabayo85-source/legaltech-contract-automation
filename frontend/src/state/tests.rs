//! Unit tests for theme logic.
//!
//! Pure logic only — no DOM access — so these run under `wasm-pack
//! test --node` without needing a browser or WebDriver.

#![cfg(test)]

use wasm_bindgen_test::wasm_bindgen_test;

use crate::state::theme::Theme;

#[wasm_bindgen_test]
fn theme_as_str_matches_expected_values() {
    assert_eq!(Theme::Light.as_str(), "light");
    assert_eq!(Theme::Dark.as_str(), "dark");
}

#[wasm_bindgen_test]
fn theme_toggle_flips_light_to_dark() {
    assert_eq!(Theme::Light.toggled(), Theme::Dark);
}

#[wasm_bindgen_test]
fn theme_toggle_flips_dark_to_light() {
    assert_eq!(Theme::Dark.toggled(), Theme::Light);
}

#[wasm_bindgen_test]
fn theme_toggle_is_involutive() {
    // Toggling twice returns to the original value — catches a
    // copy-paste bug in the match arms that a single-toggle test
    // would miss.
    assert_eq!(Theme::Light.toggled().toggled(), Theme::Light);
    assert_eq!(Theme::Dark.toggled().toggled(), Theme::Dark);
}
