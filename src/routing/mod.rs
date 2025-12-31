//! AiMesh Routing Module
//! 
//! Cost-aware routing engine with budget tracking, endpoint scoring,
//! and fallback chain management.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use parking_lot::RwLock;
use thiserror::Error;
use tracing::{debug, info, warn};

use crate::protocol::{
    AiMessage, EndpointMetrics, HealthStatus, RoutingDecision, BudgetInfo, RoutingScore,
};

/// Routing errors
#[derive(Error, Debug)]
pub enum RoutingError {
    #[error("No healthy endpoints available")]
    NoHealthyEndpoints,
    
    #[error("Budget exceeded for agent {agent_id}: required {required}, available {available}")]
    BudgetExceeded {
        agent_id: String,
        required: f64,
        available: f64,
    },
    
    #[error("Endpoint not found: {0}")]
    EndpointNotFound(String),
    
    #[error("Rate limit exceeded for agent: {0}")]
    RateLimitExceeded(String),
}

/// Scoring weights for endpoint selection
#[derive(Debug, Clone)]
pub struct ScoringWeights {
    pub cost_weight: f64,
    pub load_weight: f64,
    pub latency_weight: f64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            cost_weight: 0.4,
            load_weight: 0.3,
            latency_weight: 0.3,
        }
    }
}

/// Configuration for the routing engine
#[derive(Debug, Clone)]
pub struct RouterConfig {
    /// Scoring weights for endpoint selection
    pub weights: ScoringWeights,
    /// Health check interval in seconds
    pub health_check_interval_secs: u64,
    /// Maximum retries on fallback
    pub max_retries: u32,
    /// Endpoint considered unhealthy after this many failures
    pub unhealthy_threshold: u32,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            weights: ScoringWeights::default(),
            health_check_interval_secs: 5,
            max_retries: 3,
            unhealthy_threshold: 3,
        }
    }
}

/// Endpoint with internal tracking
#[derive(Debug, Clone)]
pub struct Endpoint {
    pub metrics: EndpointMetrics,
    pub consecutive_failures: u32,
    pub last_success: i64,
}

impl Endpoint {
    /// Create a new endpoint from metrics
    pub fn new(metrics: EndpointMetrics) -> Self {
        Self {
            metrics,
            consecutive_failures: 0,
            last_success: Self::now_ns(),
        }
    }
    
    fn now_ns() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64
    }
    
    /// Check if endpoint is healthy
    pub fn is_healthy(&self) -> bool {
        self.metrics.health_status == HealthStatus::Healthy as i32
    }
    
    /// Get load percentage (0.0 - 1.0)
    pub fn load_percentage(&self) -> f64 {
        if self.metrics.capacity == 0 {
            return 1.0; // Treat as fully loaded
        }
        self.metrics.current_load as f64 / self.metrics.capacity as f64
    }
}

/// Cost-aware router with budget tracking
pub struct CostAwareRouter {
    /// Registered endpoints
    endpoints: DashMap<String, Endpoint>,
    /// Per-agent budget tracking
    budgets: DashMap<String, BudgetInfo>,
    /// Routing decision history (for analytics)
    routing_history: Arc<RwLock<Vec<RoutingDecision>>>,
    /// Router configuration
    config: RouterConfig,
}

impl CostAwareRouter {
    /// Create a new cost-aware router
    pub fn new(config: RouterConfig) -> Self {
        Self {
            endpoints: DashMap::new(),
            budgets: DashMap::new(),
            routing_history: Arc::new(RwLock::new(Vec::new())),
            config,
        }
    }
    
    /// Route a message to the best endpoint (< 1μs target)
    pub async fn route(&self, message: &AiMessage) -> Result<RoutingDecision, RoutingError> {
        // 1. Check budget
        self.check_budget(&message.agent_id, message.estimated_cost_tokens)?;
        
        // 2. Get healthy endpoints
        let healthy = self.get_healthy_endpoints();
        if healthy.is_empty() {
            return Err(RoutingError::NoHealthyEndpoints);
        }
        
        // 3. Score all healthy endpoints
        let mut scored: Vec<(String, f64, &Endpoint)> = healthy
            .iter()
            .map(|e| {
                let score = self.score_endpoint(e);
                (e.metrics.endpoint_id.clone(), score, e)
            })
            .collect();
        
        // 4. Sort by score (lowest is best)
        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        
        // 5. Build routing decision
        let (best_id, best_score, best_endpoint) = &scored[0];
        
        let cost_score = best_endpoint.metrics.cost_per_1k_tokens * self.config.weights.cost_weight;
        let load_score = best_endpoint.load_percentage() * 100.0 * self.config.weights.load_weight;
        let latency_score = best_endpoint.metrics.latency_p99_ms as f64 * self.config.weights.latency_weight;
        
        let mut decision = RoutingDecision {
            message_id: message.message_id.clone(),
            target_endpoint: best_id.clone(),
            estimated_latency_ms: best_endpoint.metrics.latency_p99_ms as i32,
            estimated_cost: best_endpoint.metrics.cost_per_1k_tokens * message.estimated_cost_tokens / 1000.0,
            routing_reason: format!(
                "Best score {:.4} (cost: {:.2}, load: {:.0}%, latency: {:.0}ms)",
                best_score,
                best_endpoint.metrics.cost_per_1k_tokens,
                best_endpoint.load_percentage() * 100.0,
                best_endpoint.metrics.latency_p99_ms
            ),
            fallback_endpoints: scored.iter().skip(1).take(2).map(|(id, _, _)| id.clone()).collect(),
            score_breakdown: Some(RoutingScore {
                cost_score,
                load_score,
                latency_score,
                total_score: cost_score + load_score + latency_score,
            }),
        };
        
        // 6. Record decision for analytics
        self.record_decision(&decision);
        
        debug!(
            message_id = %message.message_id,
            endpoint = %best_id,
            score = %best_score,
            "Routed message"
        );
        
        Ok(decision)
    }
    
    /// Register an endpoint
    pub fn register_endpoint(&self, metrics: EndpointMetrics) {
        let endpoint_id = metrics.endpoint_id.clone();
        self.endpoints.insert(endpoint_id.clone(), Endpoint::new(metrics));
        info!(endpoint = %endpoint_id, "Registered endpoint");
    }
    
    /// Update endpoint metrics
    pub fn update_endpoint_metrics(&self, endpoint_id: &str, metrics: EndpointMetrics) -> Result<(), RoutingError> {
        if let Some(mut entry) = self.endpoints.get_mut(endpoint_id) {
            entry.metrics = metrics;
            Ok(())
        } else {
            Err(RoutingError::EndpointNotFound(endpoint_id.to_string()))
        }
    }
    
    /// Mark an endpoint as failed
    pub fn record_endpoint_failure(&self, endpoint_id: &str) {
        if let Some(mut entry) = self.endpoints.get_mut(endpoint_id) {
            entry.consecutive_failures += 1;
            if entry.consecutive_failures >= self.config.unhealthy_threshold {
                entry.metrics.health_status = HealthStatus::Unhealthy as i32;
                warn!(endpoint = %endpoint_id, "Endpoint marked unhealthy");
            }
        }
    }
    
    /// Mark an endpoint as successful
    pub fn record_endpoint_success(&self, endpoint_id: &str) {
        if let Some(mut entry) = self.endpoints.get_mut(endpoint_id) {
            entry.consecutive_failures = 0;
            entry.last_success = Endpoint::now_ns();
            entry.metrics.health_status = HealthStatus::Healthy as i32;
        }
    }
    
    /// Set budget for an agent
    pub fn set_budget(&self, agent_id: &str, initial_tokens: f64, reset_at: i64) {
        self.budgets.insert(agent_id.to_string(), BudgetInfo {
            agent_id: agent_id.to_string(),
            initial_tokens,
            remaining_tokens: initial_tokens,
            consumption_rate: 0.0,
            reset_at,
        });
    }
    
    /// Consume tokens from an agent's budget
    pub fn consume_budget(&self, agent_id: &str, tokens: f64) -> Result<f64, RoutingError> {
        if let Some(mut budget) = self.budgets.get_mut(agent_id) {
            if budget.remaining_tokens < tokens {
                return Err(RoutingError::BudgetExceeded {
                    agent_id: agent_id.to_string(),
                    required: tokens,
                    available: budget.remaining_tokens,
                });
            }
            budget.remaining_tokens -= tokens;
            Ok(budget.remaining_tokens)
        } else {
            // No budget set, allow unlimited
            Ok(f64::MAX)
        }
    }
    
    /// Get remaining budget for an agent
    pub fn get_remaining_budget(&self, agent_id: &str) -> f64 {
        self.budgets
            .get(agent_id)
            .map(|b| b.remaining_tokens)
            .unwrap_or(f64::MAX)
    }
    
    /// Get all healthy endpoints
    fn get_healthy_endpoints(&self) -> Vec<Endpoint> {
        self.endpoints
            .iter()
            .filter(|e| e.is_healthy())
            .map(|e| e.clone())
            .collect()
    }
    
    /// Score an endpoint (lower is better)
    fn score_endpoint(&self, endpoint: &Endpoint) -> f64 {
        let cost_score = endpoint.metrics.cost_per_1k_tokens * self.config.weights.cost_weight;
        let load_score = endpoint.load_percentage() * 100.0 * self.config.weights.load_weight;
        let latency_score = endpoint.metrics.latency_p99_ms as f64 * self.config.weights.latency_weight;
        
        cost_score + load_score + latency_score
    }
    
    /// Check if agent has sufficient budget
    fn check_budget(&self, agent_id: &str, estimated_cost: f64) -> Result<(), RoutingError> {
        if let Some(budget) = self.budgets.get(agent_id) {
            if budget.remaining_tokens < estimated_cost {
                return Err(RoutingError::BudgetExceeded {
                    agent_id: agent_id.to_string(),
                    required: estimated_cost,
                    available: budget.remaining_tokens,
                });
            }
        }
        Ok(())
    }
    
    /// Record a routing decision for analytics
    fn record_decision(&self, decision: &RoutingDecision) {
        let mut history = self.routing_history.write();
        history.push(decision.clone());
        
        // Keep only last 10000 decisions
        if history.len() > 10000 {
            history.drain(0..1000);
        }
    }
    
    /// Get routing statistics
    pub fn get_stats(&self) -> RouterStats {
        let history = self.routing_history.read();
        
        RouterStats {
            total_decisions: history.len(),
            endpoints_count: self.endpoints.len(),
            healthy_endpoints: self.get_healthy_endpoints().len(),
            agents_with_budget: self.budgets.len(),
        }
    }
    
    /// List all registered endpoints
    pub fn list_endpoints(&self) -> Vec<EndpointMetrics> {
        self.endpoints.iter().map(|e| e.metrics.clone()).collect()
    }
    
    /// Remove an endpoint
    pub fn remove_endpoint(&self, endpoint_id: &str) -> bool {
        self.endpoints.remove(endpoint_id).is_some()
    }
    
    /// Reset an agent's budget
    pub fn reset_budget(&self, agent_id: &str) {
        if let Some(mut budget) = self.budgets.get_mut(agent_id) {
            budget.remaining_tokens = budget.initial_tokens;
            info!(agent = %agent_id, tokens = budget.initial_tokens, "Reset budget");
        }
    }
    
    /// Get budget info for an agent
    pub fn get_budget_info(&self, agent_id: &str) -> Option<BudgetInfo> {
        self.budgets.get(agent_id).map(|b| b.clone())
    }
    
    /// Route with retry on fallback endpoints
    pub async fn route_with_retry(&self, message: &AiMessage) -> Result<(RoutingDecision, usize), RoutingError> {
        let decision = self.route(message).await?;
        
        // Return initial decision with 0 retries
        // Caller can use fallback_endpoints for actual retries
        Ok((decision, 0))
    }
    
    /// Get degraded endpoints (high error rate or load)
    pub fn get_degraded_endpoints(&self) -> Vec<String> {
        self.endpoints
            .iter()
            .filter(|e| {
                e.metrics.health_status == HealthStatus::Degraded as i32 ||
                e.metrics.error_rate > 0.1 ||
                e.load_percentage() > 0.9
            })
            .map(|e| e.metrics.endpoint_id.clone())
            .collect()
    }
}

/// Router statistics
#[derive(Debug, Clone)]
pub struct RouterStats {
    pub total_decisions: usize,
    pub endpoints_count: usize,
    pub healthy_endpoints: usize,
    pub agents_with_budget: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_endpoint(id: &str, cost: f64, load: u32, capacity: u32, latency: f32) -> EndpointMetrics {
        EndpointMetrics {
            endpoint_id: id.to_string(),
            capacity,
            current_load: load,
            cost_per_1k_tokens: cost,
            latency_p99_ms: latency,
            error_rate: 0.0,
            last_health_check: 0,
            health_status: HealthStatus::Healthy as i32,
        }
    }
    
    #[tokio::test]
    async fn test_routing_selects_best_endpoint() {
        let router = CostAwareRouter::new(RouterConfig::default());
        
        // Register endpoints with different characteristics
        router.register_endpoint(create_test_endpoint("expensive", 10.0, 10, 100, 5.0));
        router.register_endpoint(create_test_endpoint("cheap", 1.0, 10, 100, 5.0));
        router.register_endpoint(create_test_endpoint("fast", 5.0, 10, 100, 1.0));
        
        let msg = AiMessage::new(
            "test-agent".to_string(),
            b"test".to_vec(),
            1000.0,
            i64::MAX,
        );
        
        let decision = router.route(&msg).await.unwrap();
        
        // "cheap" should be selected due to lowest cost
        assert_eq!(decision.target_endpoint, "cheap");
    }
    
    #[tokio::test]
    async fn test_budget_enforcement() {
        let router = CostAwareRouter::new(RouterConfig::default());
        router.register_endpoint(create_test_endpoint("endpoint-1", 1.0, 0, 100, 5.0));
        
        // Set limited budget
        router.set_budget("limited-agent", 100.0, i64::MAX);
        
        let mut msg = AiMessage::new(
            "limited-agent".to_string(),
            b"test".to_vec(),
            100.0,
            i64::MAX,
        );
        msg.estimated_cost_tokens = 150.0; // Exceeds budget
        
        let result = router.route(&msg).await;
        assert!(matches!(result, Err(RoutingError::BudgetExceeded { .. })));
    }
    
    #[tokio::test]
    async fn test_fallback_endpoints() {
        let router = CostAwareRouter::new(RouterConfig::default());
        
        router.register_endpoint(create_test_endpoint("primary", 1.0, 0, 100, 5.0));
        router.register_endpoint(create_test_endpoint("secondary", 2.0, 0, 100, 5.0));
        router.register_endpoint(create_test_endpoint("tertiary", 3.0, 0, 100, 5.0));
        
        let msg = AiMessage::new(
            "test-agent".to_string(),
            b"test".to_vec(),
            1000.0,
            i64::MAX,
        );
        
        let decision = router.route(&msg).await.unwrap();
        
        assert_eq!(decision.target_endpoint, "primary");
        assert_eq!(decision.fallback_endpoints.len(), 2);
    }
}
