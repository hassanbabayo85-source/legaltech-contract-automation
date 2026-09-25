//! Structured logging / tracing setup.
//!
//! Log verbosity is controlled by the standard `RUST_LOG` environment
//! variable (e.g. `RUST_LOG=info`, `RUST_LOG=lexhack_backend=debug,tower_http=info`).
//! Defaults to `info` if `RUST_LOG` is not set.
//!
//! Callers must never pass secret values (passwords, API keys, tokens, full
//! contract contents) into tracing fields. See [`crate::config`] and
//! [`crate::errors`] for how sensitive values are kept out of logs.

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry};

/// Initializes the global tracing subscriber.
///
/// Must be called exactly once, as early as possible in `main`, before any
/// other startup step that might log.
pub fn init() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer().with_target(true);

    Registry::default().with(env_filter).with(fmt_layer).init();
}
