//! AiMesh Observability Module
//!
//! Metrics collection, tracing, and dashboard support for monitoring
//! the AI message queue performance and costs.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use tracing::{info, debug};

/// Metric types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
}

/// A single metric value with percentiles
#[derive(Debug, Clone, Default)]
pub struct MetricValue {
    pub count: u64,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    pub p50: f64,
    pub p99: f64,
    pub p999: f64,
}

/// Histogram for latency tracking
#[derive(Debug)]
pub struct Histogram {
    values: parking_lot::RwLock<Vec<f64>>,
    count: AtomicU64,
}

impl Histogram {
    pub fn new() -> Self {
        Self {
            values: parking_lot::RwLock::new(Vec::with_capacity(10000)),
            count: AtomicU64::new(0),
        }
    }
    
    pub fn record(&self, value: f64) {
        self.values.write().push(value);
        self.count.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn snapshot(&self) -> MetricValue {
        let mut values = self.values.read().clone();
        if values.is_empty() {
            return MetricValue::default();
        }
        
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let len = values.len();
        
        MetricValue {
            count: len as u64,
            sum: values.iter().sum(),
            min: values[0],
            max: values[len - 1],
            p50: values[len / 2],
            p99: values[(len as f64 * 0.99) as usize],
            p999: values[(len as f64 * 0.999).min((len - 1) as f64) as usize],
        }
    }
    
    pub fn clear(&self) {
        self.values.write().clear();
        self.count.store(0, Ordering::Relaxed);
    }
}

/// Counter metric
#[derive(Debug)]
pub struct Counter {
    value: AtomicU64,
}

impl Counter {
    pub fn new() -> Self {
        Self { value: AtomicU64::new(0) }
    }
    
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn add(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }
    
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

/// Observability layer for metrics and tracing
pub struct ObservabilityLayer {
    /// Message throughput counter
    pub messages_total: Counter,
    /// Messages by status
    pub messages_success: Counter,
    pub messages_failed: Counter,
    /// Routing latency histogram
    pub routing_latency_us: Histogram,
    /// End-to-end latency histogram
    pub e2e_latency_ms: Histogram,
    /// Cost tracking
    pub tokens_consumed: AtomicU64,
    pub total_cost_cents: AtomicU64,
    /// Per-agent metrics
    agent_tokens: DashMap<String, AtomicU64>,
    agent_messages: DashMap<String, AtomicU64>,
    /// Start time for uptime tracking
    start_time: Instant,
}

impl ObservabilityLayer {
    pub fn new() -> Self {
        info!("Initializing observability layer");
        Self {
            messages_total: Counter::new(),
            messages_success: Counter::new(),
            messages_failed: Counter::new(),
            routing_latency_us: Histogram::new(),
            e2e_latency_ms: Histogram::new(),
            tokens_consumed: AtomicU64::new(0),
            total_cost_cents: AtomicU64::new(0),
            agent_tokens: DashMap::new(),
            agent_messages: DashMap::new(),
            start_time: Instant::now(),
        }
    }
    
    /// Record a message processed
    pub fn record_message(&self, agent_id: &str, success: bool, latency_ms: f64, tokens: f64, cost_cents: f64) {
        self.messages_total.inc();
        
        if success {
            self.messages_success.inc();
        } else {
            self.messages_failed.inc();
        }
        
        self.e2e_latency_ms.record(latency_ms);
        self.tokens_consumed.fetch_add(tokens as u64, Ordering::Relaxed);
        self.total_cost_cents.fetch_add((cost_cents * 100.0) as u64, Ordering::Relaxed);
        
        // Per-agent tracking
        self.agent_tokens
            .entry(agent_id.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(tokens as u64, Ordering::Relaxed);
        
        self.agent_messages
            .entry(agent_id.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record routing decision latency
    pub fn record_routing_latency(&self, latency_us: f64) {
        self.routing_latency_us.record(latency_us);
    }
    
    /// Get current stats
    pub fn get_stats(&self) -> ObservabilityStats {
        ObservabilityStats {
            uptime_secs: self.start_time.elapsed().as_secs(),
            messages_total: self.messages_total.get(),
            messages_success: self.messages_success.get(),
            messages_failed: self.messages_failed.get(),
            tokens_consumed: self.tokens_consumed.load(Ordering::Relaxed),
            total_cost_cents: self.total_cost_cents.load(Ordering::Relaxed),
            routing_latency: self.routing_latency_us.snapshot(),
            e2e_latency: self.e2e_latency_ms.snapshot(),
            throughput_per_sec: self.calculate_throughput(),
        }
    }
    
    /// Get per-agent stats
    pub fn get_agent_stats(&self, agent_id: &str) -> AgentStats {
        let tokens = self.agent_tokens
            .get(agent_id)
            .map(|v| v.load(Ordering::Relaxed))
            .unwrap_or(0);
        
        let messages = self.agent_messages
            .get(agent_id)
            .map(|v| v.load(Ordering::Relaxed))
            .unwrap_or(0);
        
        AgentStats { agent_id: agent_id.to_string(), tokens_consumed: tokens, messages_processed: messages }
    }
    
    fn calculate_throughput(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.messages_total.get() as f64 / elapsed
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone)]
pub struct ObservabilityStats {
    pub uptime_secs: u64,
    pub messages_total: u64,
    pub messages_success: u64,
    pub messages_failed: u64,
    pub tokens_consumed: u64,
    pub total_cost_cents: u64,
    pub routing_latency: MetricValue,
    pub e2e_latency: MetricValue,
    pub throughput_per_sec: f64,
}

#[derive(Debug, Clone)]
pub struct AgentStats {
    pub agent_id: String,
    pub tokens_consumed: u64,
    pub messages_processed: u64,
}

impl Default for ObservabilityLayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_counter() {
        let counter = Counter::new();
        counter.inc();
        counter.add(5);
        assert_eq!(counter.get(), 6);
    }
    
    #[test]
    fn test_histogram() {
        let hist = Histogram::new();
        for i in 1..=100 {
            hist.record(i as f64);
        }
        
        let snapshot = hist.snapshot();
        assert_eq!(snapshot.count, 100);
        assert_eq!(snapshot.min, 1.0);
        assert_eq!(snapshot.max, 100.0);
        // P50 should be around 50 (may be 50 or 51 depending on index calculation)
        assert!(snapshot.p50 >= 50.0 && snapshot.p50 <= 51.0);
    }
    
    #[test]
    fn test_observability() {
        let obs = ObservabilityLayer::new();
        obs.record_message("agent-1", true, 5.0, 100.0, 0.01);
        obs.record_message("agent-1", true, 3.0, 50.0, 0.005);
        
        let stats = obs.get_stats();
        assert_eq!(stats.messages_total, 2);
        assert_eq!(stats.messages_success, 2);
        assert_eq!(stats.tokens_consumed, 150);
    }
}
