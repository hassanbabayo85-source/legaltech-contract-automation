//! Route-level page components.
//!
//! Every page is a thin composition of `components` and `api`. Pages
//! never render untrusted content as HTML — see
//! `components::SafeText`.

pub mod channels;
pub mod contracts;
pub mod dashboard;
pub mod login;
pub mod register;
pub mod reminders;
pub mod settings;
