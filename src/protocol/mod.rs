//! AiMesh Protocol Module
//! 
//! Provides message serialization, deserialization, and validation
//! for the AiMesh high-performance AI message queue.

use std::time::{SystemTime, UNIX_EPOCH};
use prost::Message;
use thiserror::Error;

// Include the generated protobuf code
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/aimesh.rs"));
}

pub use proto::*;

/// Protocol errors
#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("Serialization failed: {0}")]
    SerializationError(String),
    
    #[error("Deserialization failed: {0}")]
    DeserializationFailed(String),
    
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("Message too large: {size} bytes (max: {max} bytes)")]
    MessageTooLarge { size: usize, max: usize },
    
    #[error("Invalid agent ID: {0}")]
    InvalidAgentId(String),
    
    #[error("Budget exceeded: required {required}, available {available}")]
    BudgetExceeded { required: f64, available: f64 },
    
    #[error("Deadline expired: deadline was {deadline_ms}ms, current time is {current_ms}ms")]
    DeadlineExpired { deadline_ms: i64, current_ms: i64 },
}

/// Maximum payload size (1MB)
pub const MAX_PAYLOAD_SIZE: usize = 1024 * 1024;

/// Validation rules for agent IDs
const AGENT_ID_PATTERN: &str = r"^[a-z0-9_-]+$";

impl AiMessage {
    /// Create a new AIMessage with required fields
    pub fn new(
        agent_id: String,
        payload: Vec<u8>,
        budget_tokens: f64,
        deadline_ms: i64,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64;
        
        Self {
            agent_id,
            message_id: uuid::Uuid::now_v7().to_string(),
            payload,
            estimated_cost_tokens: 0.0,
            budget_tokens,
            deadline_ms,
            task_graph_id: String::new(),
            dependencies: Vec::new(),
            priority: 50, // Default medium priority
            dedup_context: String::new(),
            trace_id: uuid::Uuid::now_v7().to_string(),
            metadata: std::collections::HashMap::new(),
            timestamp: now,
        }
    }
    
    /// Serialize the message to bytes (< 100ns target)
    #[inline]
    pub fn serialize(&self) -> Result<Vec<u8>, ProtocolError> {
        let mut buf = Vec::with_capacity(self.encoded_len());
        self.encode(&mut buf)
            .map_err(|e| ProtocolError::SerializationError(e.to_string()))?;
        Ok(buf)
    }
    
    /// Deserialize from bytes (< 100ns target)
    #[inline]
    pub fn deserialize(data: &[u8]) -> Result<Self, ProtocolError> {
        Self::decode(data)
            .map_err(|e| ProtocolError::DeserializationFailed(e.to_string()))
    }
    
    /// Validate the message
    pub fn validate(&self) -> Result<(), ProtocolError> {
        // Validate agent_id format
        if self.agent_id.is_empty() {
            return Err(ProtocolError::InvalidAgentId("Agent ID cannot be empty".into()));
        }
        
        // Check agent_id matches pattern [a-z0-9_-]+
        let valid = self.agent_id.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-'
        });
        if !valid {
            return Err(ProtocolError::InvalidAgentId(
                format!("Agent ID '{}' must match pattern {}", self.agent_id, AGENT_ID_PATTERN)
            ));
        }
        
        // Validate payload size
        if self.payload.len() > MAX_PAYLOAD_SIZE {
            return Err(ProtocolError::MessageTooLarge {
                size: self.payload.len(),
                max: MAX_PAYLOAD_SIZE,
            });
        }
        
        // Validate budget is positive
        if self.budget_tokens <= 0.0 {
            return Err(ProtocolError::ValidationFailed(
                "budget_tokens must be positive".into()
            ));
        }
        
        // Validate deadline is in the future
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        
        if self.deadline_ms > 0 && self.deadline_ms < now_ms {
            return Err(ProtocolError::DeadlineExpired {
                deadline_ms: self.deadline_ms,
                current_ms: now_ms,
            });
        }
        
        // Validate priority range
        if self.priority < 0 || self.priority > 100 {
            return Err(ProtocolError::ValidationFailed(
                format!("Priority must be 0-100, got {}", self.priority)
            ));
        }
        
        Ok(())
    }
    
    /// Check if this message has exceeded its budget
    pub fn is_over_budget(&self, estimated_cost: f64) -> bool {
        estimated_cost > self.budget_tokens
    }
    
    /// Check if this message has expired
    pub fn is_expired(&self) -> bool {
        if self.deadline_ms == 0 {
            return false; // No deadline set
        }
        
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        
        self.deadline_ms < now_ms
    }
    
    /// Get the age of this message in milliseconds
    pub fn age_ms(&self) -> u64 {
        let now_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64;
        
        ((now_ns - self.timestamp) / 1_000_000) as u64
    }
}

impl RoutingDecision {
    /// Create a new routing decision
    pub fn new(
        message_id: String,
        target_endpoint: String,
        estimated_latency_ms: i32,
        estimated_cost: f64,
        routing_reason: String,
    ) -> Self {
        Self {
            message_id,
            target_endpoint,
            estimated_latency_ms,
            estimated_cost,
            routing_reason,
            fallback_endpoints: Vec::new(),
            score_breakdown: None,
        }
    }
    
    /// Add a fallback endpoint
    pub fn with_fallback(mut self, endpoint: String) -> Self {
        self.fallback_endpoints.push(endpoint);
        self
    }
    
    /// Add score breakdown for observability
    pub fn with_score(mut self, cost: f64, load: f64, latency: f64) -> Self {
        self.score_breakdown = Some(RoutingScore {
            cost_score: cost,
            load_score: load,
            latency_score: latency,
            total_score: cost + load + latency,
        });
        self
    }
}

impl AcknowledgmentMessage {
    /// Create a success acknowledgment
    pub fn success(
        original_message_id: String,
        tokens_used: f64,
        processing_latency_ms: i32,
        result: Vec<u8>,
    ) -> Self {
        Self {
            original_message_id,
            status: AckStatus::AckProcessed as i32,
            tokens_used,
            processing_latency_ms,
            error: String::new(),
            result,
        }
    }
    
    /// Create a failure acknowledgment
    pub fn failure(original_message_id: String, error: String) -> Self {
        Self {
            original_message_id,
            status: AckStatus::AckFailed as i32,
            tokens_used: 0.0,
            processing_latency_ms: 0,
            error,
            result: Vec::new(),
        }
    }
    
    /// Check if this acknowledgment indicates success
    pub fn is_success(&self) -> bool {
        self.status == AckStatus::AckProcessed as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_message_serialization_roundtrip() {
        let msg = AiMessage::new(
            "test-agent".to_string(),
            b"Hello, World!".to_vec(),
            1000.0,
            i64::MAX,
        );
        
        let serialized = msg.serialize().unwrap();
        let deserialized = AiMessage::deserialize(&serialized).unwrap();
        
        assert_eq!(msg.agent_id, deserialized.agent_id);
        assert_eq!(msg.payload, deserialized.payload);
        assert_eq!(msg.budget_tokens, deserialized.budget_tokens);
    }
    
    #[test]
    fn test_message_validation() {
        let valid_msg = AiMessage::new(
            "valid-agent-123".to_string(),
            b"payload".to_vec(),
            100.0,
            i64::MAX,
        );
        assert!(valid_msg.validate().is_ok());
        
        // Invalid agent ID
        let invalid_agent = AiMessage::new(
            "INVALID_AGENT".to_string(), // uppercase not allowed
            b"payload".to_vec(),
            100.0,
            i64::MAX,
        );
        assert!(invalid_agent.validate().is_err());
    }
    
    #[test]
    fn test_routing_decision() {
        let decision = RoutingDecision::new(
            "msg-123".to_string(),
            "endpoint-a".to_string(),
            5,
            0.001,
            "Lowest cost".to_string(),
        )
        .with_fallback("endpoint-b".to_string())
        .with_score(0.4, 0.3, 0.3);
        
        assert_eq!(decision.fallback_endpoints.len(), 1);
        assert!(decision.score_breakdown.is_some());
    }
}
