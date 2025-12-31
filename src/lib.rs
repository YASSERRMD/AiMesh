//! AiMesh - High-Performance AI Agent Message Queue
//!
//! A cost-aware message routing system for AI agents with:
//! - QUIC transport for low-latency communication
//! - Cost-aware routing with budget enforcement
//! - Semantic deduplication
//! - Scatter-gather orchestration
//! - Multi-tier storage (local + Barq-GraphDB)

pub mod protocol;
pub mod routing;
pub mod storage;
pub mod observability;
pub mod dedup;
pub mod orchestration;
pub mod transport;
pub mod ratelimit;
pub mod tenant;
pub mod priority;

use std::sync::Arc;
use thiserror::Error;

pub use protocol::*;
pub use routing::{CostAwareRouter, RouterConfig, RoutingError};
pub use storage::{StorageLayer, StorageConfig, StorageBackend, StorageError};
pub use observability::ObservabilityLayer;
pub use ratelimit::{RateLimiter, RateLimitConfig, RateLimitError};

/// AiMesh errors
#[derive(Error, Debug)]
pub enum AiMeshError {
    #[error("Protocol error: {0}")]
    Protocol(#[from] protocol::ProtocolError),
    #[error("Routing error: {0}")]
    Routing(#[from] routing::RoutingError),
    #[error("Storage error: {0}")]
    Storage(#[from] storage::StorageError),
    #[error("Transport error: {0}")]
    Transport(String),
    #[error("Rate limit error: {0}")]
    RateLimit(#[from] ratelimit::RateLimitError),
    #[error("Configuration error: {0}")]
    Config(String),
}

/// AiMesh configuration
#[derive(Debug, Clone)]
pub struct AiMeshConfig {
    /// QUIC server bind address
    pub bind_addr: String,
    /// Router configuration
    pub router: RouterConfig,
    /// Storage configuration
    pub storage: StorageConfig,
    /// Rate limit configuration
    pub rate_limit: RateLimitConfig,
    /// Enable semantic deduplication
    pub enable_dedup: bool,
    /// Dedup cache TTL in seconds
    pub dedup_ttl_secs: u64,
    /// Enable rate limiting
    pub enable_rate_limit: bool,
}

impl Default for AiMeshConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:8080".into(),
            router: RouterConfig::default(),
            storage: StorageConfig::default(),
            rate_limit: RateLimitConfig::default(),
            enable_dedup: true,
            dedup_ttl_secs: 3600,
            enable_rate_limit: true,
        }
    }
}

/// Main AiMesh message queue instance
pub struct AiMesh {
    pub config: AiMeshConfig,
    pub router: Arc<CostAwareRouter>,
    pub storage: Arc<StorageLayer>,
    pub observability: Arc<ObservabilityLayer>,
    pub rate_limiter: Arc<RateLimiter>,
}

impl AiMesh {
    /// Create a new AiMesh instance
    pub fn new(config: AiMeshConfig) -> Result<Self, AiMeshError> {
        let router = Arc::new(CostAwareRouter::new(config.router.clone()));
        let storage = Arc::new(StorageLayer::new(config.storage.clone())?);
        let observability = Arc::new(ObservabilityLayer::new());
        let rate_limiter = Arc::new(RateLimiter::new(config.rate_limit.clone()));
        
        Ok(Self {
            config,
            router,
            storage,
            observability,
            rate_limiter,
        })
    }
    
    /// Process a message through the queue
    pub async fn process_message(&self, message: AiMessage) -> Result<AcknowledgmentMessage, AiMeshError> {
        let start = std::time::Instant::now();
        
        // 1. Validate message
        message.validate()?;
        
        // 2. Check rate limit
        if self.config.enable_rate_limit {
            self.rate_limiter.acquire(&message.agent_id)?;
        }
        
        // 2. Check for duplicates (if enabled)
        if self.config.enable_dedup {
            let hash = compute_dedup_hash(&message);
            if let Some(cached) = self.storage.check_dedup(&hash) {
                return Ok(AcknowledgmentMessage::success(
                    message.message_id.clone(),
                    0.0, // No tokens used for cached response
                    start.elapsed().as_millis() as i32,
                    cached,
                ));
            }
        }
        
        // 3. Route the message
        let routing_start = std::time::Instant::now();
        let decision = self.router.route(&message).await?;
        self.observability.record_routing_latency(routing_start.elapsed().as_micros() as f64);
        
        // 4. Store the message
        self.storage.write_message(&message).await?;
        
        // 5. TODO: Send to target endpoint via QUIC transport
        // For now, return a placeholder acknowledgment
        let result = vec![]; // Placeholder for actual response
        
        // 6. Cache for dedup
        if self.config.enable_dedup {
            let hash = compute_dedup_hash(&message);
            self.storage.write_dedup(&hash, result.clone());
        }
        
        // 7. Update budget
        self.router.consume_budget(&message.agent_id, decision.estimated_cost)?;
        
        // 8. Record metrics
        let latency_ms = start.elapsed().as_millis() as f64;
        self.observability.record_message(
            &message.agent_id,
            true,
            latency_ms,
            decision.estimated_cost,
            decision.estimated_cost * 0.001, // Cost estimate
        );
        
        Ok(AcknowledgmentMessage::success(
            message.message_id,
            decision.estimated_cost,
            latency_ms as i32,
            result,
        ))
    }
    
    /// Get system statistics
    pub fn get_stats(&self) -> SystemStats {
        SystemStats {
            observability: self.observability.get_stats(),
            router: self.router.get_stats(),
        }
    }
}

/// System-wide statistics
#[derive(Debug)]
pub struct SystemStats {
    pub observability: observability::ObservabilityStats,
    pub router: routing::RouterStats,
}

/// Compute a deduplication hash for a message
fn compute_dedup_hash(message: &AiMessage) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    message.payload.hash(&mut hasher);
    message.dedup_context.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
