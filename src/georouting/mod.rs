//! AiMesh Geo-Routing Module
//!
//! Latency-based geographic routing with region affinity and failover.

use std::collections::HashMap;
use std::sync::Arc;
use dashmap::DashMap;
use tracing::{debug, info};

use crate::federation::{GeoLocation, Region, Peer, FederationManager};
use crate::routing::CostAwareRouter;
use crate::protocol::AiMessage;

/// Geo-routing configuration
#[derive(Debug, Clone)]
pub struct GeoRoutingConfig {
    /// Enable latency-based routing
    pub latency_based: bool,
    /// Maximum acceptable latency in ms
    pub max_latency_ms: u32,
    /// Enable region affinity
    pub region_affinity: bool,
    /// Fallback to any region if local unavailable
    pub allow_fallback: bool,
    /// Weight for latency in routing decisions
    pub latency_weight: f64,
    /// Weight for cost in routing decisions
    pub cost_weight: f64,
    /// Weight for load in routing decisions
    pub load_weight: f64,
}

impl Default for GeoRoutingConfig {
    fn default() -> Self {
        Self {
            latency_based: true,
            max_latency_ms: 500,
            region_affinity: true,
            allow_fallback: true,
            latency_weight: 0.4,
            cost_weight: 0.3,
            load_weight: 0.3,
        }
    }
}

/// Geo-aware routing decision
#[derive(Debug, Clone)]
pub struct GeoRoutingDecision {
    pub target_region: String,
    pub target_endpoint: String,
    pub estimated_latency_ms: u32,
    pub estimated_cost: f64,
    pub routing_reason: String,
    pub is_local: bool,
    pub fallback_regions: Vec<String>,
}

/// Geo-routing engine
pub struct GeoRouter {
    config: GeoRoutingConfig,
    federation: Arc<FederationManager>,
    local_router: Arc<CostAwareRouter>,
    /// Cached latency measurements per region
    region_latencies: DashMap<String, LatencyStats>,
    /// Client locations for affinity
    client_regions: DashMap<String, String>,
}

/// Latency statistics for a region
#[derive(Debug, Clone)]
struct LatencyStats {
    samples: Vec<u32>,
    avg_ms: u32,
    p99_ms: u32,
    last_updated: std::time::Instant,
}

impl Default for LatencyStats {
    fn default() -> Self {
        Self {
            samples: Vec::new(),
            avg_ms: 0,
            p99_ms: 0,
            last_updated: std::time::Instant::now(),
        }
    }
}

impl LatencyStats {
    fn record(&mut self, latency_ms: u32) {
        self.samples.push(latency_ms);
        if self.samples.len() > 1000 {
            self.samples.remove(0);
        }
        self.update_stats();
    }
    
    fn update_stats(&mut self) {
        if self.samples.is_empty() {
            return;
        }
        
        let sum: u32 = self.samples.iter().sum();
        self.avg_ms = sum / self.samples.len() as u32;
        
        let mut sorted = self.samples.clone();
        sorted.sort();
        let p99_idx = (sorted.len() as f64 * 0.99) as usize;
        self.p99_ms = sorted.get(p99_idx).copied().unwrap_or(self.avg_ms);
        
        self.last_updated = std::time::Instant::now();
    }
}

impl GeoRouter {
    pub fn new(
        config: GeoRoutingConfig,
        federation: Arc<FederationManager>,
        local_router: Arc<CostAwareRouter>,
    ) -> Self {
        Self {
            config,
            federation,
            local_router,
            region_latencies: DashMap::new(),
            client_regions: DashMap::new(),
        }
    }
    
    /// Route a message with geo-awareness
    pub async fn route(&self, message: &AiMessage, client_location: Option<&GeoLocation>) -> Result<GeoRoutingDecision, GeoRoutingError> {
        // Determine target region
        let target_region = self.determine_target_region(message, client_location)?;
        
        // Check if local region
        let local_region = self.federation.get_stats().local_region;
        let is_local = target_region == local_region;
        
        if is_local {
            // Use local router
            let decision = self.local_router.route(message).await
                .map_err(|e| GeoRoutingError::RoutingFailed(e.to_string()))?;
            
            return Ok(GeoRoutingDecision {
                target_region: local_region,
                target_endpoint: decision.target_endpoint,
                estimated_latency_ms: decision.estimated_latency_ms as u32,
                estimated_cost: decision.estimated_cost,
                routing_reason: format!("Local routing: {}", decision.routing_reason),
                is_local: true,
                fallback_regions: vec![],
            });
        }
        
        // Route to remote region
        self.route_to_remote_region(message, &target_region)
    }
    
    /// Determine target region based on message metadata and client location
    fn determine_target_region(&self, message: &AiMessage, client_location: Option<&GeoLocation>) -> Result<String, GeoRoutingError> {
        // Check if message specifies a region
        if let Some(region) = message.metadata.get("target_region") {
            return Ok(region.clone());
        }
        
        // Check client region affinity
        if self.config.region_affinity {
            if let Some(region) = self.client_regions.get(&message.agent_id) {
                return Ok(region.clone());
            }
        }
        
        // Use client location to find nearest region
        if let Some(location) = client_location {
            if let Some(region) = self.federation.get_nearest_region(location) {
                return Ok(region.id);
            }
        }
        
        // Default to local region
        Ok(self.federation.get_stats().local_region)
    }
    
    /// Route to a remote region
    fn route_to_remote_region(&self, message: &AiMessage, target_region: &str) -> Result<GeoRoutingDecision, GeoRoutingError> {
        // Get routing path
        let path = self.federation.route_to_region(target_region)
            .map_err(|e| GeoRoutingError::RegionUnavailable(e.to_string()))?;
        
        if path.hops.is_empty() {
            return Err(GeoRoutingError::NoRoute(target_region.into()));
        }
        
        let peer = &path.hops[0];
        
        // Check latency constraint
        if self.config.latency_based && peer.latency_ms > self.config.max_latency_ms {
            if !self.config.allow_fallback {
                return Err(GeoRoutingError::LatencyExceeded {
                    region: target_region.into(),
                    latency_ms: peer.latency_ms,
                    max_ms: self.config.max_latency_ms,
                });
            }
            // Find fallback region
            return self.find_fallback_region(message);
        }
        
        // Build fallback list
        let fallback_regions: Vec<String> = self.federation.list_regions()
            .iter()
            .filter(|r| r.id != target_region)
            .map(|r| r.id.clone())
            .take(3)
            .collect();
        
        Ok(GeoRoutingDecision {
            target_region: target_region.into(),
            target_endpoint: peer.address.clone(),
            estimated_latency_ms: peer.latency_ms,
            estimated_cost: 0.0, // Remote cost TBD
            routing_reason: format!(
                "Geo-routed to {} via peer {} (latency: {}ms)",
                target_region, peer.id, peer.latency_ms
            ),
            is_local: false,
            fallback_regions,
        })
    }
    
    /// Find a fallback region when target is unavailable
    fn find_fallback_region(&self, _message: &AiMessage) -> Result<GeoRoutingDecision, GeoRoutingError> {
        let local_region = self.federation.get_stats().local_region;
        
        // Try to route locally
        // Note: This is synchronous fallback, actual routing would be async
        Ok(GeoRoutingDecision {
            target_region: local_region.clone(),
            target_endpoint: "local".into(),
            estimated_latency_ms: 0,
            estimated_cost: 0.0,
            routing_reason: "Fallback to local region".into(),
            is_local: true,
            fallback_regions: vec![],
        })
    }
    
    /// Set region affinity for a client/agent
    pub fn set_client_region(&self, agent_id: &str, region_id: &str) {
        self.client_regions.insert(agent_id.to_string(), region_id.to_string());
        info!(agent = %agent_id, region = %region_id, "Set client region affinity");
    }
    
    /// Get client's preferred region
    pub fn get_client_region(&self, agent_id: &str) -> Option<String> {
        self.client_regions.get(agent_id).map(|r| r.clone())
    }
    
    /// Record latency sample for a region
    pub fn record_latency(&self, region_id: &str, latency_ms: u32) {
        let mut stats = self.region_latencies
            .entry(region_id.to_string())
            .or_insert_with(|| LatencyStats {
                samples: Vec::new(),
                avg_ms: 0,
                p99_ms: 0,
                last_updated: std::time::Instant::now(),
            });
        stats.record(latency_ms);
    }
    
    /// Get latency stats for a region
    pub fn get_latency_stats(&self, region_id: &str) -> Option<(u32, u32)> {
        self.region_latencies.get(region_id)
            .map(|s| (s.avg_ms, s.p99_ms))
    }
    
    /// Get routing stats
    pub fn get_stats(&self) -> GeoRoutingStats {
        GeoRoutingStats {
            tracked_clients: self.client_regions.len(),
            tracked_regions: self.region_latencies.len(),
            config: self.config.clone(),
        }
    }
}

/// Geo-routing errors
#[derive(Debug, thiserror::Error)]
pub enum GeoRoutingError {
    #[error("Region unavailable: {0}")]
    RegionUnavailable(String),
    #[error("No route to region: {0}")]
    NoRoute(String),
    #[error("Latency exceeded for region {region}: {latency_ms}ms > {max_ms}ms")]
    LatencyExceeded {
        region: String,
        latency_ms: u32,
        max_ms: u32,
    },
    #[error("Routing failed: {0}")]
    RoutingFailed(String),
}

/// Geo-routing statistics
#[derive(Debug, Clone)]
pub struct GeoRoutingStats {
    pub tracked_clients: usize,
    pub tracked_regions: usize,
    pub config: GeoRoutingConfig,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::federation::FederationConfig;
    use crate::routing::RouterConfig;
    
    #[test]
    fn test_latency_stats() {
        let mut stats = LatencyStats::default();
        
        for i in 1..=100 {
            stats.record(i);
        }
        
        assert_eq!(stats.samples.len(), 100);
        assert!(stats.avg_ms > 0);
        assert!(stats.p99_ms >= stats.avg_ms);
    }
    
    #[test]
    fn test_client_region_affinity() {
        let federation = Arc::new(FederationManager::new(FederationConfig::default()));
        let router = Arc::new(CostAwareRouter::new(RouterConfig::default()));
        
        let geo_router = GeoRouter::new(
            GeoRoutingConfig::default(),
            federation,
            router,
        );
        
        geo_router.set_client_region("agent-1", "us-west-1");
        
        let region = geo_router.get_client_region("agent-1");
        assert_eq!(region, Some("us-west-1".to_string()));
    }
}
