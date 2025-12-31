//! AiMesh Federation Module
//!
//! Multi-region federation with peer discovery, message forwarding,
//! and cross-cluster coordination.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use dashmap::DashMap;
use parking_lot::RwLock;
use thiserror::Error;
use tracing::{debug, info, warn, error};

#[derive(Error, Debug)]
pub enum FederationError {
    #[error("Peer not found: {0}")]
    PeerNotFound(String),
    #[error("Region not found: {0}")]
    RegionNotFound(String),
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Forwarding failed: {0}")]
    ForwardingFailed(String),
    #[error("Cluster unhealthy: {0}")]
    ClusterUnhealthy(String),
}

/// Region identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Region {
    pub id: String,
    pub name: String,
    pub location: GeoLocation,
}

/// Geographic location
#[derive(Debug, Clone, PartialEq)]
pub struct GeoLocation {
    pub latitude: f64,
    pub longitude: f64,
    pub country: String,
    pub city: String,
}

impl GeoLocation {
    pub fn new(lat: f64, lon: f64, country: &str, city: &str) -> Self {
        Self {
            latitude: lat,
            longitude: lon,
            country: country.into(),
            city: city.into(),
        }
    }
    
    /// Calculate distance in kilometers using Haversine formula
    pub fn distance_to(&self, other: &GeoLocation) -> f64 {
        const R: f64 = 6371.0; // Earth radius in km
        
        let lat1 = self.latitude.to_radians();
        let lat2 = other.latitude.to_radians();
        let dlat = (other.latitude - self.latitude).to_radians();
        let dlon = (other.longitude - self.longitude).to_radians();
        
        let a = (dlat / 2.0).sin().powi(2) +
                lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().asin();
        
        R * c
    }
}

impl Eq for GeoLocation {}

impl std::hash::Hash for GeoLocation {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.country.hash(state);
        self.city.hash(state);
    }
}

/// Peer node in the federation
#[derive(Debug, Clone)]
pub struct Peer {
    pub id: String,
    pub address: String,
    pub region: Region,
    pub status: PeerStatus,
    pub last_heartbeat: Instant,
    pub latency_ms: u32,
    pub capacity: u32,
    pub current_load: u32,
}

impl Peer {
    pub fn new(id: String, address: String, region: Region) -> Self {
        Self {
            id,
            address,
            region,
            status: PeerStatus::Unknown,
            last_heartbeat: Instant::now(),
            latency_ms: 0,
            capacity: 1000,
            current_load: 0,
        }
    }
    
    pub fn is_healthy(&self) -> bool {
        matches!(self.status, PeerStatus::Healthy) &&
        self.last_heartbeat.elapsed() < Duration::from_secs(30)
    }
    
    pub fn load_percentage(&self) -> f64 {
        if self.capacity == 0 {
            return 1.0;
        }
        self.current_load as f64 / self.capacity as f64
    }
}

/// Peer status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerStatus {
    Unknown,
    Healthy,
    Degraded,
    Unhealthy,
    Unreachable,
}

/// Federation configuration
#[derive(Debug, Clone)]
pub struct FederationConfig {
    /// Local cluster ID
    pub cluster_id: String,
    /// Local region
    pub region: Region,
    /// Heartbeat interval in seconds
    pub heartbeat_interval_secs: u64,
    /// Peer timeout in seconds
    pub peer_timeout_secs: u64,
    /// Enable cross-region routing
    pub cross_region_routing: bool,
    /// Prefer local region
    pub prefer_local: bool,
    /// Max hops for message forwarding
    pub max_forward_hops: u8,
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            cluster_id: "cluster-1".into(),
            region: Region {
                id: "us-east-1".into(),
                name: "US East".into(),
                location: GeoLocation::new(39.0, -77.0, "US", "Virginia"),
            },
            heartbeat_interval_secs: 10,
            peer_timeout_secs: 30,
            cross_region_routing: true,
            prefer_local: true,
            max_forward_hops: 3,
        }
    }
}

/// Federation manager for multi-cluster coordination
pub struct FederationManager {
    config: FederationConfig,
    /// Known peers by ID
    peers: DashMap<String, Peer>,
    /// Peers by region
    peers_by_region: DashMap<String, Vec<String>>,
    /// Region metadata
    regions: DashMap<String, Region>,
    /// Routing table: destination -> next hop peer
    routing_table: Arc<RwLock<HashMap<String, String>>>,
}

impl FederationManager {
    pub fn new(config: FederationConfig) -> Self {
        // Register local region
        let local_region = config.region.clone();
        let manager = Self {
            config,
            peers: DashMap::new(),
            peers_by_region: DashMap::new(),
            regions: DashMap::new(),
            routing_table: Arc::new(RwLock::new(HashMap::new())),
        };
        
        manager.regions.insert(local_region.id.clone(), local_region);
        manager
    }
    
    /// Register a peer node
    pub fn register_peer(&self, peer: Peer) {
        let region_id = peer.region.id.clone();
        let peer_id = peer.id.clone();
        
        // Add region if new
        if !self.regions.contains_key(&region_id) {
            self.regions.insert(region_id.clone(), peer.region.clone());
        }
        
        // Add to peers
        self.peers.insert(peer_id.clone(), peer);
        
        // Add to region index
        self.peers_by_region
            .entry(region_id.clone())
            .or_insert_with(Vec::new)
            .push(peer_id.clone());
        
        info!(peer_id = %peer_id, region = %region_id, "Registered peer");
    }
    
    /// Update peer status
    pub fn update_peer_status(&self, peer_id: &str, status: PeerStatus, latency_ms: u32) {
        if let Some(mut peer) = self.peers.get_mut(peer_id) {
            peer.status = status;
            peer.latency_ms = latency_ms;
            peer.last_heartbeat = Instant::now();
            debug!(peer_id = %peer_id, status = ?status, latency = latency_ms, "Updated peer");
        }
    }
    
    /// Update peer load
    pub fn update_peer_load(&self, peer_id: &str, current_load: u32) {
        if let Some(mut peer) = self.peers.get_mut(peer_id) {
            peer.current_load = current_load;
        }
    }
    
    /// Remove a peer
    pub fn remove_peer(&self, peer_id: &str) -> bool {
        if let Some((_, peer)) = self.peers.remove(peer_id) {
            // Remove from region index
            if let Some(mut peers) = self.peers_by_region.get_mut(&peer.region.id) {
                peers.retain(|id| id != peer_id);
            }
            info!(peer_id = %peer_id, "Removed peer");
            true
        } else {
            false
        }
    }
    
    /// Get best peer for a target region
    pub fn get_best_peer(&self, target_region: &str) -> Option<Peer> {
        let peer_ids = self.peers_by_region.get(target_region)?;
        
        // Score and select best peer
        let mut best: Option<(f64, Peer)> = None;
        
        for peer_id in peer_ids.iter() {
            if let Some(peer) = self.peers.get(peer_id) {
                if !peer.is_healthy() {
                    continue;
                }
                
                // Score: lower is better
                let score = peer.latency_ms as f64 * 0.5 +
                           peer.load_percentage() * 100.0 * 0.5;
                
                if best.is_none() || score < best.as_ref().unwrap().0 {
                    best = Some((score, peer.clone()));
                }
            }
        }
        
        best.map(|(_, peer)| peer)
    }
    
    /// Get nearest region to a location
    pub fn get_nearest_region(&self, location: &GeoLocation) -> Option<Region> {
        let mut nearest: Option<(f64, Region)> = None;
        
        for entry in self.regions.iter() {
            let distance = location.distance_to(&entry.location);
            
            if nearest.is_none() || distance < nearest.as_ref().unwrap().0 {
                nearest = Some((distance, entry.clone()));
            }
        }
        
        nearest.map(|(_, region)| region)
    }
    
    /// Route to destination, optionally forwarding through peers
    pub fn route_to_region(&self, target_region: &str) -> Result<RoutingPath, FederationError> {
        // Check if target is local
        if target_region == self.config.region.id {
            return Ok(RoutingPath {
                hops: vec![],
                target_region: target_region.into(),
                estimated_latency_ms: 0,
                is_local: true,
            });
        }
        
        if !self.config.cross_region_routing {
            return Err(FederationError::RegionNotFound(target_region.into()));
        }
        
        // Find direct peer in target region
        if let Some(peer) = self.get_best_peer(target_region) {
            return Ok(RoutingPath {
                hops: vec![peer.clone()],
                target_region: target_region.into(),
                estimated_latency_ms: peer.latency_ms,
                is_local: false,
            });
        }
        
        // Check routing table for multi-hop
        let routing_table = self.routing_table.read();
        if let Some(next_hop) = routing_table.get(target_region) {
            if let Some(peer) = self.peers.get(next_hop) {
                return Ok(RoutingPath {
                    hops: vec![peer.clone()],
                    target_region: target_region.into(),
                    estimated_latency_ms: peer.latency_ms,
                    is_local: false,
                });
            }
        }
        
        Err(FederationError::RegionNotFound(target_region.into()))
    }
    
    /// Get all healthy peers
    pub fn get_healthy_peers(&self) -> Vec<Peer> {
        self.peers.iter()
            .filter(|p| p.is_healthy())
            .map(|p| p.clone())
            .collect()
    }
    
    /// Get peers in a region
    pub fn get_peers_in_region(&self, region_id: &str) -> Vec<Peer> {
        self.peers_by_region.get(region_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.peers.get(id).map(|p| p.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }
    
    /// Get all regions
    pub fn list_regions(&self) -> Vec<Region> {
        self.regions.iter().map(|r| r.clone()).collect()
    }
    
    /// Get cluster stats
    pub fn get_stats(&self) -> FederationStats {
        let total_peers = self.peers.len();
        let healthy_peers = self.peers.iter().filter(|p| p.is_healthy()).count();
        let total_regions = self.regions.len();
        
        FederationStats {
            cluster_id: self.config.cluster_id.clone(),
            local_region: self.config.region.id.clone(),
            total_peers,
            healthy_peers,
            total_regions,
            cross_region_enabled: self.config.cross_region_routing,
        }
    }
}

/// Routing path to destination
#[derive(Debug, Clone)]
pub struct RoutingPath {
    pub hops: Vec<Peer>,
    pub target_region: String,
    pub estimated_latency_ms: u32,
    pub is_local: bool,
}

/// Federation statistics
#[derive(Debug, Clone)]
pub struct FederationStats {
    pub cluster_id: String,
    pub local_region: String,
    pub total_peers: usize,
    pub healthy_peers: usize,
    pub total_regions: usize,
    pub cross_region_enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_region(id: &str, lat: f64, lon: f64) -> Region {
        Region {
            id: id.into(),
            name: id.into(),
            location: GeoLocation::new(lat, lon, "US", "Test"),
        }
    }
    
    #[test]
    fn test_geo_distance() {
        let nyc = GeoLocation::new(40.7128, -74.0060, "US", "New York");
        let la = GeoLocation::new(34.0522, -118.2437, "US", "Los Angeles");
        
        let distance = nyc.distance_to(&la);
        // NYC to LA is ~3940 km
        assert!(distance > 3900.0 && distance < 4000.0);
    }
    
    #[test]
    fn test_peer_registration() {
        let config = FederationConfig::default();
        let manager = FederationManager::new(config);
        
        let peer = Peer::new(
            "peer-1".into(),
            "10.0.0.1:9000".into(),
            create_test_region("us-west-1", 37.0, -122.0),
        );
        
        manager.register_peer(peer);
        
        assert_eq!(manager.peers.len(), 1);
        assert!(manager.peers.contains_key("peer-1"));
    }
    
    #[test]
    fn test_nearest_region() {
        let config = FederationConfig::default();
        let manager = FederationManager::new(config);
        
        // Add regions
        manager.regions.insert(
            "us-east-1".into(),
            create_test_region("us-east-1", 39.0, -77.0),
        );
        manager.regions.insert(
            "us-west-1".into(),
            create_test_region("us-west-1", 37.0, -122.0),
        );
        manager.regions.insert(
            "eu-west-1".into(),
            create_test_region("eu-west-1", 53.0, -8.0),
        );
        
        // Find nearest to NYC
        let nyc = GeoLocation::new(40.7128, -74.0060, "US", "New York");
        let nearest = manager.get_nearest_region(&nyc).unwrap();
        assert_eq!(nearest.id, "us-east-1");
    }
    
    #[test]
    fn test_local_routing() {
        let config = FederationConfig::default();
        let manager = FederationManager::new(config.clone());
        
        let path = manager.route_to_region(&config.region.id).unwrap();
        assert!(path.is_local);
        assert!(path.hops.is_empty());
    }
}
