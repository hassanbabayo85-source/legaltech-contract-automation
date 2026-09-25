//! Small, generic helpers that don't belong to a specific layer.
//!
//! Currently contains [`redact`] — best-effort secret redaction for values
//! that are about to be written to logs. This is defense-in-depth: code
//! that handles secrets should still avoid putting them into log messages
//! in the first place.

pub mod redact;
