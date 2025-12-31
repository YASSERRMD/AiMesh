//! AiMesh Rate Limiting Module
//!
//! Token bucket and sliding window rate limiters for fair resource allocation.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use dashmap::DashMap;
use thiserror::Error;
use tracing::{debug, warn};

#[derive(Error, Debug)]
pub enum RateLimitError {
    #[error("Rate limit exceeded for {key}: {limit} requests per {window_secs}s")]
    LimitExceeded {
        key: String,
        limit: u64,
        window_secs: u64,
    },
    #[error("Quota exhausted for {key}")]
    QuotaExhausted { key: String },
}

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Requests per second limit
    pub requests_per_second: u64,
    /// Burst capacity (token bucket size)
    pub burst_capacity: u64,
    /// Sliding window duration in seconds
    pub window_secs: u64,
    /// Enable adaptive rate limiting
    pub adaptive: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 100,
            burst_capacity: 200,
            window_secs: 60,
            adaptive: true,
        }
    }
}

/// Token bucket state
#[derive(Debug)]
struct TokenBucket {
    tokens: AtomicU64,
    last_refill: parking_lot::Mutex<Instant>,
    capacity: u64,
    refill_rate: u64, // tokens per second
}

impl TokenBucket {
    fn new(capacity: u64, refill_rate: u64) -> Self {
        Self {
            tokens: AtomicU64::new(capacity),
            last_refill: parking_lot::Mutex::new(Instant::now()),
            capacity,
            refill_rate,
        }
    }
    
    fn try_acquire(&self, count: u64) -> bool {
        self.refill();
        
        loop {
            let current = self.tokens.load(Ordering::Relaxed);
            if current < count {
                return false;
            }
            
            if self.tokens.compare_exchange(
                current,
                current - count,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ).is_ok() {
                return true;
            }
        }
    }
    
    fn refill(&self) {
        let mut last = self.last_refill.lock();
        let now = Instant::now();
        let elapsed = now.duration_since(*last);
        
        if elapsed.as_millis() > 0 {
            let new_tokens = (elapsed.as_millis() as u64 * self.refill_rate) / 1000;
            if new_tokens > 0 {
                let current = self.tokens.load(Ordering::Relaxed);
                let new_value = (current + new_tokens).min(self.capacity);
                self.tokens.store(new_value, Ordering::Relaxed);
                *last = now;
            }
        }
    }
    
    fn available(&self) -> u64 {
        self.refill();
        self.tokens.load(Ordering::Relaxed)
    }
}

/// Sliding window counter
#[derive(Debug)]
struct SlidingWindow {
    counts: parking_lot::RwLock<Vec<(Instant, u64)>>,
    window_duration: Duration,
    limit: u64,
}

impl SlidingWindow {
    fn new(window_secs: u64, limit: u64) -> Self {
        Self {
            counts: parking_lot::RwLock::new(Vec::new()),
            window_duration: Duration::from_secs(window_secs),
            limit,
        }
    }
    
    fn try_acquire(&self, count: u64) -> bool {
        let now = Instant::now();
        let cutoff = now - self.window_duration;
        
        let mut counts = self.counts.write();
        
        // Remove expired entries
        counts.retain(|(time, _)| *time > cutoff);
        
        // Calculate total in window
        let total: u64 = counts.iter().map(|(_, c)| c).sum();
        
        if total + count <= self.limit {
            counts.push((now, count));
            true
        } else {
            false
        }
    }
    
    fn current_count(&self) -> u64 {
        let now = Instant::now();
        let cutoff = now - self.window_duration;
        
        let counts = self.counts.read();
        counts.iter()
            .filter(|(time, _)| *time > cutoff)
            .map(|(_, c)| c)
            .sum()
    }
}

/// Combined rate limiter with token bucket and sliding window
pub struct RateLimiter {
    config: RateLimitConfig,
    /// Per-key token buckets
    buckets: DashMap<String, TokenBucket>,
    /// Per-key sliding windows
    windows: DashMap<String, SlidingWindow>,
    /// Global token bucket
    global_bucket: TokenBucket,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        let global_bucket = TokenBucket::new(
            config.burst_capacity * 10, // 10x for global
            config.requests_per_second * 10,
        );
        
        Self {
            config,
            buckets: DashMap::new(),
            windows: DashMap::new(),
            global_bucket,
        }
    }
    
    /// Check if request is allowed (does not consume)
    pub fn check(&self, key: &str) -> bool {
        // Check global limit
        if self.global_bucket.available() == 0 {
            return false;
        }
        
        // Check per-key bucket
        if let Some(bucket) = self.buckets.get(key) {
            if bucket.available() == 0 {
                return false;
            }
        }
        
        // Check sliding window
        if let Some(window) = self.windows.get(key) {
            if window.current_count() >= self.config.requests_per_second * self.config.window_secs {
                return false;
            }
        }
        
        true
    }
    
    /// Try to acquire a request slot
    pub fn acquire(&self, key: &str) -> Result<(), RateLimitError> {
        self.acquire_n(key, 1)
    }
    
    /// Try to acquire N request slots
    pub fn acquire_n(&self, key: &str, count: u64) -> Result<(), RateLimitError> {
        // Check global limit first
        if !self.global_bucket.try_acquire(count) {
            warn!(key = %key, "Global rate limit hit");
            return Err(RateLimitError::LimitExceeded {
                key: "global".to_string(),
                limit: self.config.requests_per_second * 10,
                window_secs: 1,
            });
        }
        
        // Get or create per-key bucket
        let bucket = self.buckets.entry(key.to_string()).or_insert_with(|| {
            TokenBucket::new(self.config.burst_capacity, self.config.requests_per_second)
        });
        
        if !bucket.try_acquire(count) {
            debug!(key = %key, "Per-key rate limit hit");
            return Err(RateLimitError::LimitExceeded {
                key: key.to_string(),
                limit: self.config.requests_per_second,
                window_secs: 1,
            });
        }
        
        // Update sliding window
        let window = self.windows.entry(key.to_string()).or_insert_with(|| {
            SlidingWindow::new(
                self.config.window_secs,
                self.config.requests_per_second * self.config.window_secs,
            )
        });
        
        if !window.try_acquire(count) {
            return Err(RateLimitError::LimitExceeded {
                key: key.to_string(),
                limit: self.config.requests_per_second * self.config.window_secs,
                window_secs: self.config.window_secs,
            });
        }
        
        Ok(())
    }
    
    /// Get current usage for a key
    pub fn get_usage(&self, key: &str) -> RateLimitUsage {
        let bucket_available = self.buckets.get(key)
            .map(|b| b.available())
            .unwrap_or(self.config.burst_capacity);
        
        let window_count = self.windows.get(key)
            .map(|w| w.current_count())
            .unwrap_or(0);
        
        RateLimitUsage {
            tokens_available: bucket_available,
            window_count,
            window_limit: self.config.requests_per_second * self.config.window_secs,
        }
    }
    
    /// Reset rate limit for a key
    pub fn reset(&self, key: &str) {
        self.buckets.remove(key);
        self.windows.remove(key);
    }
    
    /// Get all rate-limited keys
    pub fn get_limited_keys(&self) -> Vec<String> {
        self.buckets.iter()
            .filter(|entry| entry.available() == 0)
            .map(|entry| entry.key().clone())
            .collect()
    }
}

/// Rate limit usage information
#[derive(Debug, Clone)]
pub struct RateLimitUsage {
    pub tokens_available: u64,
    pub window_count: u64,
    pub window_limit: u64,
}

impl RateLimitUsage {
    pub fn utilization_percent(&self) -> f64 {
        if self.window_limit == 0 {
            return 0.0;
        }
        (self.window_count as f64 / self.window_limit as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_token_bucket() {
        let bucket = TokenBucket::new(10, 5);
        
        // Should have full capacity initially
        assert!(bucket.try_acquire(5));
        assert!(bucket.try_acquire(5));
        
        // Should be empty now
        assert!(!bucket.try_acquire(1));
    }
    
    #[test]
    fn test_rate_limiter() {
        let config = RateLimitConfig {
            requests_per_second: 10,
            burst_capacity: 20,
            window_secs: 1,
            adaptive: false,
        };
        
        let limiter = RateLimiter::new(config);
        
        // Should allow burst
        for _ in 0..20 {
            assert!(limiter.acquire("test-key").is_ok());
        }
        
        // Should be rate limited now
        assert!(limiter.acquire("test-key").is_err());
    }
    
    #[test]
    fn test_usage_tracking() {
        let config = RateLimitConfig::default();
        let limiter = RateLimiter::new(config);
        
        limiter.acquire("key1").unwrap();
        limiter.acquire("key1").unwrap();
        
        let usage = limiter.get_usage("key1");
        assert_eq!(usage.window_count, 2);
    }
}
