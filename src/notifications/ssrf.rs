//! SSRF protection for outbound webhook requests.
//!
//! Every webhook URL — regardless of who configured it — is validated
//! before the request is made:
//!
//! 1. Only `https://` is accepted. `http://` and every other scheme
//!    (`file://`, `ftp://`, `gopher://`, `unix://`, ...) is rejected.
//! 2. The hostname is resolved via the system resolver.
//! 3. **Every** resolved IP address must pass the blocklist below. If
//!    any address is blocked, the request is refused.
//!
//!    Validation alone is not sufficient: a name that returned a
//!    public address during validation could return a private one
//!    during the connection. Callers must therefore pin the HTTP
//!    client to the exact addresses returned by
//!    [`validate_and_resolve`] via
//!    [`crate::notifications::http::build_pinned_client`]. See
//!    [`validate_and_resolve`] for the recommended entry point.
//!
//! The blocklist covers:
//!
//! * Loopback (`127.0.0.0/8`, `::1`)
//! * Private IPv4 (`10/8`, `172.16/12`, `192.168/16`)
//! * IPv6 unique-local (`fc00::/7`)
//! * Link-local (`169.254/16`, `fe80::/10`) — this also covers the
//!   cloud metadata endpoint `169.254.169.254`
//! * Unspecified addresses (`0.0.0.0`, `::`)
//! * IPv4 broadcast (`255.255.255.255`)
//!
//! # Test-only escape hatch
//!
//! `allow_loopback = true` relaxes the loopback rule so that
//! integration tests can point at a local mock server. **It must never
//! be enabled in production.** It is read from
//! `WEBHOOK_ALLOW_LOOPBACK`, whose default is `false`.

use std::net::{IpAddr, Ipv6Addr, ToSocketAddrs};

use reqwest::Url;

use crate::notifications::channel::DeliveryError;

/// Validates that `url` is safe to POST to **and** returns the
/// exact set of resolved IP addresses that were checked.
///
/// The returned addresses are intended to be passed to
/// [`crate::notifications::http::build_pinned_client`] so the request
/// is issued only to those addresses — closing the DNS-rebinding
/// window between validation and connection.
pub fn validate_and_resolve(
    url: &Url,
    allow_loopback: bool,
) -> Result<Vec<std::net::SocketAddr>, DeliveryError> {
    // 1. Scheme check.
    match url.scheme() {
        "https" => {}
        "http" if allow_loopback => {} // test-only escape hatch
        _ => return Err(DeliveryError::UnsafeDestination),
    }

    // 2. Use `Url::host` so IPv6 literals arrive as parsed addresses.
    let host = url.host().ok_or(DeliveryError::InvalidUrl)?;
    let port = url.port_or_known_default().unwrap_or(443);

    match host {
        url::Host::Ipv4(v4) => {
            let addr = std::net::SocketAddr::new(IpAddr::V4(v4), port);
            if !is_public(addr.ip(), allow_loopback) {
                return Err(DeliveryError::UnsafeDestination);
            }
            Ok(vec![addr])
        }
        url::Host::Ipv6(v6) => {
            let addr = std::net::SocketAddr::new(IpAddr::V6(v6), port);
            if !is_public(addr.ip(), allow_loopback) {
                return Err(DeliveryError::UnsafeDestination);
            }
            Ok(vec![addr])
        }
        url::Host::Domain(name) => {
            let addrs: Vec<std::net::SocketAddr> = (name, port)
                .to_socket_addrs()
                .map_err(|_| DeliveryError::InvalidUrl)?
                .collect();

            if addrs.is_empty() {
                return Err(DeliveryError::InvalidUrl);
            }
            for addr in &addrs {
                if !is_public(addr.ip(), allow_loopback) {
                    return Err(DeliveryError::UnsafeDestination);
                }
            }
            Ok(addrs)
        }
    }
}

/// Validates that `url` is safe to POST to.
///
/// Returns `Ok(())` if the URL is `https://` and every resolved address
/// is public. Returns a `DeliveryError` with an appropriate category
/// otherwise.
///
/// **New code should prefer [`validate_and_resolve`]** and pin the
/// returned addresses via
/// [`crate::notifications::http::build_pinned_client`]. Calling this
/// wrapper alone is not sufficient to prevent DNS rebinding.
///
/// `allow_loopback` relaxes the loopback rule; everything else remains
/// enforced. It exists only for the test suite.
pub fn validate(url: &Url, allow_loopback: bool) -> Result<(), DeliveryError> {
    // 1. Scheme check.
    match url.scheme() {
        "https" => {}
        "http" if allow_loopback => {} // test-only escape hatch
        _ => return Err(DeliveryError::UnsafeDestination),
    }

    // 2. Use `Url::host` (not `host_str`) so an IPv6 literal is
    //    delivered to us as a parsed `Ipv6Addr` rather than as the
    //    bracketed textual form `[::1]`. `to_socket_addrs` cannot parse
    //    the bracketed form.
    let host = url.host().ok_or(DeliveryError::InvalidUrl)?;

    match host {
        url::Host::Ipv4(v4) => {
            if !is_public(IpAddr::V4(v4), allow_loopback) {
                return Err(DeliveryError::UnsafeDestination);
            }
            Ok(())
        }
        url::Host::Ipv6(v6) => {
            if !is_public(IpAddr::V6(v6), allow_loopback) {
                return Err(DeliveryError::UnsafeDestination);
            }
            Ok(())
        }
        url::Host::Domain(name) => {
            let port = url.port_or_known_default().unwrap_or(443);
            let addrs = (name, port)
                .to_socket_addrs()
                .map_err(|_| DeliveryError::InvalidUrl)?;

            let mut any_checked = false;
            for addr in addrs {
                any_checked = true;
                if !is_public(addr.ip(), allow_loopback) {
                    return Err(DeliveryError::UnsafeDestination);
                }
            }

            if !any_checked {
                return Err(DeliveryError::InvalidUrl);
            }

            Ok(())
        }
    }
}

/// True if `ip` is safe to connect to.
fn is_public(ip: IpAddr, allow_loopback: bool) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            if v4.is_loopback() {
                return allow_loopback;
            }
            !(v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_documentation()
                || is_shared_v4(v4))
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() {
                return allow_loopback;
            }
            !(v6.is_unspecified() || is_unique_local_v6(v6) || is_link_local_v6(v6))
        }
    }
}

/// `100.64.0.0/10` — carrier-grade NAT, treated as internal.
fn is_shared_v4(ip: std::net::Ipv4Addr) -> bool {
    let o = ip.octets();
    o[0] == 100 && (64..=127).contains(&o[1])
}

/// `fc00::/7`.
fn is_unique_local_v6(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

/// `fe80::/10`.
fn is_link_local_v6(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).expect("test URL should parse")
    }

    #[test]
    fn rejects_plain_http() {
        let err = validate(&url("http://example.com/hook"), false).unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    #[test]
    fn rejects_file_scheme() {
        let err = validate(&url("file:///etc/passwd"), false).unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    #[test]
    fn rejects_ftp_scheme() {
        let err = validate(&url("ftp://example.com/x"), false).unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    #[test]
    fn rejects_gopher_scheme() {
        let err = validate(&url("gopher://example.com/"), false).unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    #[test]
    fn rejects_loopback_v4_by_literal() {
        let err = validate(&url("https://127.0.0.1/hook"), false).unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    #[test]
    fn rejects_loopback_v6_by_literal() {
        let err = validate(&url("https://[::1]/hook"), false).unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    #[test]
    fn rejects_private_v4_10() {
        let err = validate(&url("https://10.0.0.1/hook"), false).unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    #[test]
    fn rejects_private_v4_172_16() {
        let err = validate(&url("https://172.16.0.1/hook"), false).unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    #[test]
    fn rejects_private_v4_192_168() {
        let err = validate(&url("https://192.168.1.1/hook"), false).unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    #[test]
    fn rejects_link_local_v4_metadata() {
        let err = validate(&url("https://169.254.169.254/latest/meta-data/"), false).unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    #[test]
    fn rejects_unique_local_v6() {
        let err = validate(&url("https://[fc00::1]/hook"), false).unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    #[test]
    fn rejects_link_local_v6() {
        let err = validate(&url("https://[fe80::1]/hook"), false).unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    #[test]
    fn rejects_unspecified_v4() {
        let err = validate(&url("https://0.0.0.0/hook"), false).unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    #[test]
    fn rejects_carrier_grade_nat() {
        let err = validate(&url("https://100.64.0.1/hook"), false).unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    #[test]
    fn loopback_allowed_when_escape_hatch_enabled() {
        // Test-only. Never enable in production.
        let ok = validate(&url("http://127.0.0.1:1234/hook"), true);
        assert!(ok.is_ok());
    }

    #[test]
    fn still_rejects_metadata_with_escape_hatch() {
        // Even with the escape hatch on, link-local remains blocked.
        let err = validate(&url("https://169.254.169.254/"), true).unwrap_err();
        assert_eq!(err.safe_category(), "ssrf_blocked");
    }

    #[test]
    fn accepts_public_ip_literal() {
        // 1.1.1.1 is Cloudflare's public DNS resolver; no DNS lookup.
        assert!(validate(&url("https://1.1.1.1/hook"), false).is_ok());
    }

    #[test]
    fn is_public_classifies_representative_addresses() {
        use std::net::IpAddr;
        assert!(!is_public("127.0.0.1".parse::<IpAddr>().unwrap(), false));
        assert!(!is_public("10.0.0.1".parse::<IpAddr>().unwrap(), false));
        assert!(!is_public("192.168.1.1".parse::<IpAddr>().unwrap(), false));
        assert!(!is_public(
            "169.254.169.254".parse::<IpAddr>().unwrap(),
            false
        ));
        assert!(!is_public("::1".parse::<IpAddr>().unwrap(), false));
        assert!(!is_public("fe80::1".parse::<IpAddr>().unwrap(), false));
        assert!(!is_public("fc00::1".parse::<IpAddr>().unwrap(), false));
        assert!(is_public("1.1.1.1".parse::<IpAddr>().unwrap(), false));
        assert!(is_public("8.8.8.8".parse::<IpAddr>().unwrap(), false));
    }
}
