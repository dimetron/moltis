//! Per-webhook and global rate limiting.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::RwLock;

/// Per-webhook sliding-window rate limiter.
pub struct WebhookRateLimiter {
    windows: RwLock<HashMap<i64, VecDeque<u64>>>,
    global_window: RwLock<VecDeque<u64>>,
    global_max: u32,
}

impl WebhookRateLimiter {
    /// Create a new rate limiter with a global max requests per minute.
    pub fn new(global_max: u32) -> Self {
        Self {
            windows: RwLock::new(HashMap::new()),
            global_window: RwLock::new(VecDeque::new()),
            global_max,
        }
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// Check if a request is allowed for the given webhook.
    /// Returns `true` if allowed, `false` if rate limited.
    pub fn check(&self, webhook_id: i64, per_webhook_max: u32) -> bool {
        let now = Self::now_ms();
        let window_start = now.saturating_sub(60_000);

        // Check global limit
        {
            let mut global = self
                .global_window
                .write()
                .unwrap_or_else(|e| e.into_inner());
            while global.front().is_some_and(|&t| t < window_start) {
                global.pop_front();
            }
            if global.len() >= self.global_max as usize {
                return false;
            }
        }

        // Check per-webhook limit
        {
            let mut windows = self.windows.write().unwrap_or_else(|e| e.into_inner());
            let window = windows.entry(webhook_id).or_default();
            while window.front().is_some_and(|&t| t < window_start) {
                window.pop_front();
            }
            if window.len() >= per_webhook_max as usize {
                return false;
            }
            window.push_back(now);
        }

        // Record in global window
        {
            let mut global = self
                .global_window
                .write()
                .unwrap_or_else(|e| e.into_inner());
            global.push_back(now);
        }

        true
    }
}

impl Default for WebhookRateLimiter {
    fn default() -> Self {
        Self::new(300) // 300 global requests per minute
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_rate_limit() {
        let limiter = WebhookRateLimiter::new(100);
        // Should allow requests up to per-webhook max
        for _ in 0..5 {
            assert!(limiter.check(1, 5));
        }
        // 6th request should be rate limited
        assert!(!limiter.check(1, 5));
        // Different webhook should still be allowed
        assert!(limiter.check(2, 5));
    }
}
