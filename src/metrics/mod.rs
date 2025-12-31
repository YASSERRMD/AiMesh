//! AiMesh Metrics Module
//!
//! Prometheus-compatible metrics export and health endpoints.

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};

use crate::observability::ObservabilityLayer;
use crate::routing::CostAwareRouter;

/// Metrics configuration
#[derive(Debug, Clone)]
pub struct MetricsConfig {
    /// Metrics server bind address
    pub bind_addr: String,
    /// Enable Prometheus export
    pub prometheus_enabled: bool,
    /// Metrics path
    pub metrics_path: String,
    /// Health check path
    pub health_path: String,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:9090".into(),
            prometheus_enabled: true,
            metrics_path: "/metrics".into(),
            health_path: "/health".into(),
        }
    }
}

/// Metrics exporter
pub struct MetricsExporter {
    config: MetricsConfig,
    observability: Arc<ObservabilityLayer>,
    router: Arc<CostAwareRouter>,
    healthy: Arc<RwLock<bool>>,
}

impl MetricsExporter {
    pub fn new(
        config: MetricsConfig,
        observability: Arc<ObservabilityLayer>,
        router: Arc<CostAwareRouter>,
    ) -> Self {
        Self {
            config,
            observability,
            router,
            healthy: Arc::new(RwLock::new(true)),
        }
    }
    
    /// Generate Prometheus metrics
    pub fn prometheus_metrics(&self) -> String {
        let stats = self.observability.get_stats();
        let router_stats = self.router.get_stats();
        
        let mut output = String::new();
        
        // Message metrics
        output.push_str("# HELP aimesh_messages_total Total messages processed\n");
        output.push_str("# TYPE aimesh_messages_total counter\n");
        output.push_str(&format!("aimesh_messages_total {}\n", stats.messages_total));
        
        output.push_str("# HELP aimesh_messages_success Successful messages\n");
        output.push_str("# TYPE aimesh_messages_success counter\n");
        output.push_str(&format!("aimesh_messages_success {}\n", stats.messages_success));
        
        output.push_str("# HELP aimesh_messages_failed Failed messages\n");
        output.push_str("# TYPE aimesh_messages_failed counter\n");
        output.push_str(&format!("aimesh_messages_failed {}\n", stats.messages_failed));
        
        // Token metrics
        output.push_str("# HELP aimesh_tokens_consumed Total tokens consumed\n");
        output.push_str("# TYPE aimesh_tokens_consumed counter\n");
        output.push_str(&format!("aimesh_tokens_consumed {}\n", stats.tokens_consumed));
        
        // Cost metrics
        output.push_str("# HELP aimesh_cost_cents_total Total cost in cents\n");
        output.push_str("# TYPE aimesh_cost_cents_total counter\n");
        output.push_str(&format!("aimesh_cost_cents_total {}\n", stats.total_cost_cents));
        
        // Latency metrics
        output.push_str("# HELP aimesh_latency_ms_p50 P50 latency in milliseconds\n");
        output.push_str("# TYPE aimesh_latency_ms_p50 gauge\n");
        output.push_str(&format!("aimesh_latency_ms_p50 {:.2}\n", stats.e2e_latency.p50));
        
        output.push_str("# HELP aimesh_latency_ms_p99 P99 latency in milliseconds\n");
        output.push_str("# TYPE aimesh_latency_ms_p99 gauge\n");
        output.push_str(&format!("aimesh_latency_ms_p99 {:.2}\n", stats.e2e_latency.p99));
        
        output.push_str("# HELP aimesh_latency_ms_p999 P99.9 latency in milliseconds\n");
        output.push_str("# TYPE aimesh_latency_ms_p999 gauge\n");
        output.push_str(&format!("aimesh_latency_ms_p999 {:.2}\n", stats.e2e_latency.p999));
        
        // Routing metrics
        output.push_str("# HELP aimesh_routing_latency_us_p50 Routing P50 latency in microseconds\n");
        output.push_str("# TYPE aimesh_routing_latency_us_p50 gauge\n");
        output.push_str(&format!("aimesh_routing_latency_us_p50 {:.2}\n", stats.routing_latency.p50));
        
        output.push_str("# HELP aimesh_routing_latency_us_p99 Routing P99 latency in microseconds\n");
        output.push_str("# TYPE aimesh_routing_latency_us_p99 gauge\n");
        output.push_str(&format!("aimesh_routing_latency_us_p99 {:.2}\n", stats.routing_latency.p99));
        
        // Throughput
        output.push_str("# HELP aimesh_throughput_per_sec Messages per second\n");
        output.push_str("# TYPE aimesh_throughput_per_sec gauge\n");
        output.push_str(&format!("aimesh_throughput_per_sec {:.2}\n", stats.throughput_per_sec));
        
        // Uptime
        output.push_str("# HELP aimesh_uptime_seconds Uptime in seconds\n");
        output.push_str("# TYPE aimesh_uptime_seconds counter\n");
        output.push_str(&format!("aimesh_uptime_seconds {}\n", stats.uptime_secs));
        
        // Router metrics
        output.push_str("# HELP aimesh_endpoints_total Total registered endpoints\n");
        output.push_str("# TYPE aimesh_endpoints_total gauge\n");
        output.push_str(&format!("aimesh_endpoints_total {}\n", router_stats.endpoints_count));
        
        output.push_str("# HELP aimesh_endpoints_healthy Healthy endpoints\n");
        output.push_str("# TYPE aimesh_endpoints_healthy gauge\n");
        output.push_str(&format!("aimesh_endpoints_healthy {}\n", router_stats.healthy_endpoints));
        
        output.push_str("# HELP aimesh_routing_decisions_total Total routing decisions\n");
        output.push_str("# TYPE aimesh_routing_decisions_total counter\n");
        output.push_str(&format!("aimesh_routing_decisions_total {}\n", router_stats.total_decisions));
        
        output.push_str("# HELP aimesh_agents_with_budget Agents with configured budget\n");
        output.push_str("# TYPE aimesh_agents_with_budget gauge\n");
        output.push_str(&format!("aimesh_agents_with_budget {}\n", router_stats.agents_with_budget));
        
        output
    }
    
    /// Generate health check response
    pub async fn health_check(&self) -> HealthStatus {
        let healthy = *self.healthy.read().await;
        let stats = self.observability.get_stats();
        let router_stats = self.router.get_stats();
        
        HealthStatus {
            status: if healthy { "healthy".into() } else { "unhealthy".into() },
            uptime_secs: stats.uptime_secs,
            messages_total: stats.messages_total,
            endpoints_healthy: router_stats.healthy_endpoints,
            endpoints_total: router_stats.endpoints_count,
        }
    }
    
    /// Set healthy status
    pub async fn set_healthy(&self, healthy: bool) {
        *self.healthy.write().await = healthy;
    }
    
    /// Get metrics as JSON
    pub fn metrics_json(&self) -> serde_json::Value {
        let stats = self.observability.get_stats();
        let router_stats = self.router.get_stats();
        
        serde_json::json!({
            "messages": {
                "total": stats.messages_total,
                "success": stats.messages_success,
                "failed": stats.messages_failed,
            },
            "tokens": {
                "consumed": stats.tokens_consumed,
            },
            "cost": {
                "total_cents": stats.total_cost_cents,
            },
            "latency": {
                "e2e_p50_ms": stats.e2e_latency.p50,
                "e2e_p99_ms": stats.e2e_latency.p99,
                "e2e_p999_ms": stats.e2e_latency.p999,
                "routing_p50_us": stats.routing_latency.p50,
                "routing_p99_us": stats.routing_latency.p99,
            },
            "throughput": {
                "per_sec": stats.throughput_per_sec,
            },
            "uptime_secs": stats.uptime_secs,
            "router": {
                "endpoints_total": router_stats.endpoints_count,
                "endpoints_healthy": router_stats.healthy_endpoints,
                "decisions_total": router_stats.total_decisions,
                "agents_with_budget": router_stats.agents_with_budget,
            }
        })
    }
}

/// Health status response
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthStatus {
    pub status: String,
    pub uptime_secs: u64,
    pub messages_total: u64,
    pub endpoints_healthy: usize,
    pub endpoints_total: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::RouterConfig;
    
    #[test]
    fn test_prometheus_metrics() {
        let obs = Arc::new(ObservabilityLayer::new());
        let router = Arc::new(CostAwareRouter::new(RouterConfig::default()));
        let exporter = MetricsExporter::new(
            MetricsConfig::default(),
            obs,
            router,
        );
        
        let metrics = exporter.prometheus_metrics();
        assert!(metrics.contains("aimesh_messages_total"));
        assert!(metrics.contains("aimesh_tokens_consumed"));
        assert!(metrics.contains("aimesh_uptime_seconds"));
    }
    
    #[test]
    fn test_metrics_json() {
        let obs = Arc::new(ObservabilityLayer::new());
        let router = Arc::new(CostAwareRouter::new(RouterConfig::default()));
        let exporter = MetricsExporter::new(
            MetricsConfig::default(),
            obs,
            router,
        );
        
        let json = exporter.metrics_json();
        assert!(json.get("messages").is_some());
        assert!(json.get("latency").is_some());
        assert!(json.get("router").is_some());
    }
}
