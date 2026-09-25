//! Shared `reqwest::Client` for outbound notifications.
//!
//! One client is built at startup and shared across all channels. It
//! has redirects disabled — the SSRF policy forbids them — and enforces
//! the configured per-request timeout.

use std::time::Duration;

use reqwest::Client;

/// Builds a per-request outbound client whose DNS resolver is pinned
/// to `addrs` for `host`.
///
/// This closes the DNS-rebinding window: the request issued through
/// this client connects only to the IP addresses that were previously
/// validated by [`crate::notifications::ssrf::validate_and_resolve`].
/// A second, uncontrolled DNS lookup cannot occur.
///
/// Uses the same hardening as [`build_client`]: redirects disabled,
/// configured timeout, bounded idle pool, no proxy.
pub fn build_pinned_client(
    host: &str,
    addrs: &[std::net::SocketAddr],
    timeout: Duration,
) -> Result<Client, crate::notifications::channel::DeliveryError> {
    Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .pool_max_idle_per_host(10)
        .user_agent(concat!("lexhack-backend/", env!("CARGO_PKG_VERSION")))
        .resolve_to_addrs(host, addrs)
        .build()
        .map_err(|_| crate::notifications::channel::DeliveryError::Other)
}

/// Builds the process-wide outbound HTTP client.
///
/// Redirects are disabled entirely (`Policy::none`). Following
/// redirects would let a webhook operator bypass SSRF validation by
/// pointing at a safe-looking URL that 302s to an internal one.
pub fn build_client(request_timeout_seconds: u64) -> Client {
    Client::builder()
        .timeout(Duration::from_secs(request_timeout_seconds))
        .redirect(reqwest::redirect::Policy::none())
        .pool_max_idle_per_host(10)
        .user_agent(concat!("lexhack-backend/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("reqwest::Client construction must not fail")
}
