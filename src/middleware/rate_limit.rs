//! In-memory sliding-window rate limiter.
//!
//! Generic over the key type (`K`). The most common uses are:
//!
//! * `RateLimiter = KeyedRateLimiter<IpAddr>` — per-IP limits for
//!   unauthenticated endpoints (login, register).
//! * `UserRateLimiter = KeyedRateLimiter<Uuid>` — per-user limits for
//!   authenticated, potentially expensive endpoints (AI analysis,
//!   channel test, channel mutation).
//!
//! Per-user limits are preferable where the endpoint requires auth:
//! one user behind a shared NAT cannot exhaust another user's quota.
//!
//! # Design
//!
//! For each key, we keep a small vector of the `Instant`s at which
//! recent requests arrived. On each request, we drop entries older
//! than the window and check whether the remaining count is at or
//! above the configured maximum.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The rate limiter. Cheaply cloneable (inner state is behind `Arc`).
#[derive(Clone)]
pub struct KeyedRateLimiter<K>
where
    K: Eq + Hash + Clone,
{
    inner: Arc<Mutex<HashMap<K, Vec<Instant>>>>,
    max_requests: usize,
    window: Duration,
}

impl<K> KeyedRateLimiter<K>
where
    K: Eq + Hash + Clone,
{
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            max_requests: max_requests as usize,
            window,
        }
    }

    /// Records a request from `key`. Returns `Ok(())` if under the
    /// limit, or `Err(retry_after)` with the time until the oldest
    /// entry falls out of the window.
    pub fn check(&self, key: K) -> Result<(), Duration> {
        let now = Instant::now();
        let cutoff = now.checked_sub(self.window).unwrap_or(now);

        let mut map = self.inner.lock().expect("rate limiter mutex poisoned");

        // Opportunistic cleanup to keep memory bounded under flood.
        if map.len() > 10_000 {
            map.retain(|_, v| v.iter().any(|t| *t > cutoff));
        }

        let entries = map.entry(key).or_default();
        entries.retain(|t| *t > cutoff);

        if entries.len() >= self.max_requests {
            let oldest = entries[0];
            let retry_after = self.window.saturating_sub(now.duration_since(oldest));
            return Err(retry_after);
        }

        entries.push(now);
        Ok(())
    }
}

/// Per-IP limiter (backwards-compatible alias from PART 03).
pub type RateLimiter = KeyedRateLimiter<std::net::IpAddr>;

/// Per-user limiter. Keys are authenticated user IDs.
pub type UserRateLimiter = KeyedRateLimiter<uuid::Uuid>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;
    use uuid::Uuid;

    #[test]
    fn allows_up_to_max_then_rejects() {
        let limiter: KeyedRateLimiter<IpAddr> = KeyedRateLimiter::new(2, Duration::from_secs(60));
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        assert!(limiter.check(ip).is_ok());
        assert!(limiter.check(ip).is_ok());
        assert!(limiter.check(ip).is_err());
    }

    #[test]
    fn limits_are_per_key() {
        let limiter: KeyedRateLimiter<IpAddr> = KeyedRateLimiter::new(1, Duration::from_secs(60));
        let a: IpAddr = "10.0.0.1".parse().unwrap();
        let b: IpAddr = "10.0.0.2".parse().unwrap();
        assert!(limiter.check(a).is_ok());
        assert!(limiter.check(a).is_err());
        assert!(limiter.check(b).is_ok());
    }

    #[test]
    fn user_limiter_is_per_user() {
        let limiter: UserRateLimiter = KeyedRateLimiter::new(1, Duration::from_secs(60));
        let u1 = Uuid::new_v4();
        let u2 = Uuid::new_v4();
        assert!(limiter.check(u1).is_ok());
        assert!(
            limiter.check(u1).is_err(),
            "user 1 exhausted their own quota"
        );
        assert!(limiter.check(u2).is_ok(), "user 2 unaffected by user 1");
    }

    #[test]
    fn entry_expires_after_window() {
        let limiter: KeyedRateLimiter<IpAddr> = KeyedRateLimiter::new(1, Duration::from_millis(50));
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        assert!(limiter.check(ip).is_ok());
        assert!(limiter.check(ip).is_err());
        std::thread::sleep(Duration::from_millis(80));
        assert!(limiter.check(ip).is_ok());
    }
}
