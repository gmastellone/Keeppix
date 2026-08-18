//! In-process sliding-window rate limiter. No external dependencies beyond
//! std. Keyed by an arbitrary string (IP, token, etc.).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Per-key state: a ring of request timestamps within the window.
struct Bucket {
    timestamps: Vec<Instant>,
}

impl Bucket {
    fn new() -> Self {
        Self {
            timestamps: Vec::new(),
        }
    }

    fn prune(&mut self, window: Duration, now: Instant) {
        let cutoff = now.checked_sub(window).unwrap_or_else(Instant::now);
        self.timestamps.retain(|t| *t > cutoff);
    }

    fn count(&self) -> usize {
        self.timestamps.len()
    }

    fn record(&mut self, now: Instant) {
        self.timestamps.push(now);
    }
}

#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<RateLimiterInner>>,
    window: Duration,
    max_requests: usize,
}

struct RateLimiterInner {
    buckets: HashMap<String, Bucket>,
    last_cleanup: Instant,
}

impl RateLimiter {
    #[must_use]
    pub fn new(window: Duration, max_requests: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RateLimiterInner {
                buckets: HashMap::new(),
                last_cleanup: Instant::now(),
            })),
            window,
            max_requests,
        }
    }

    /// Returns `true` if the request is allowed, `false` if rate-limited.
    pub fn check_and_record(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Periodic cleanup of stale entries (every 60s).
        if now.duration_since(inner.last_cleanup) > Duration::from_secs(60) {
            inner.buckets.retain(|_, b| {
                b.prune(self.window, now);
                b.count() > 0
            });
            inner.last_cleanup = now;
        }

        let bucket = inner
            .buckets
            .entry(key.to_owned())
            .or_insert_with(Bucket::new);
        bucket.prune(self.window, now);

        if bucket.count() >= self.max_requests {
            return false;
        }
        bucket.record(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_max() {
        let rl = RateLimiter::new(Duration::from_secs(60), 3);
        assert!(rl.check_and_record("k"));
        assert!(rl.check_and_record("k"));
        assert!(rl.check_and_record("k"));
        assert!(!rl.check_and_record("k"));
    }

    #[test]
    fn different_keys_are_independent() {
        let rl = RateLimiter::new(Duration::from_secs(60), 1);
        assert!(rl.check_and_record("a"));
        assert!(rl.check_and_record("b"));
        assert!(!rl.check_and_record("a"));
    }
}
