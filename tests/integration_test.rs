//! Integration tests for AiMesh

use aimesh::{
    AiMesh, AiMeshConfig, AiMessage, 
    EndpointMetrics, HealthStatus,
    StorageConfig,
};

/// Test that AiMesh can be created with default config
#[tokio::test]
async fn test_aimesh_creation() {
    let mesh = AiMesh::new(AiMeshConfig::default());
    assert!(mesh.is_ok());
}

/// Test endpoint registration and routing
#[tokio::test]
async fn test_endpoint_registration() {
    let mesh = AiMesh::new(AiMeshConfig::default()).unwrap();
    
    mesh.router.register_endpoint(EndpointMetrics {
        endpoint_id: "test-endpoint".into(),
        capacity: 100,
        current_load: 0,
        cost_per_1k_tokens: 10.0,
        latency_p99_ms: 50.0,
        error_rate: 0.0,
        last_health_check: 0,
        health_status: HealthStatus::Healthy as i32,
    });
    
    let endpoints = mesh.router.list_endpoints();
    assert_eq!(endpoints.len(), 1);
    assert_eq!(endpoints[0].endpoint_id, "test-endpoint");
}

/// Test budget management
#[tokio::test]
async fn test_budget_management() {
    let mesh = AiMesh::new(AiMeshConfig::default()).unwrap();
    
    // Set budget
    mesh.router.set_budget("test-agent", 1000.0, i64::MAX);
    
    // Check budget
    let remaining = mesh.router.get_remaining_budget("test-agent");
    assert_eq!(remaining, 1000.0);
    
    // Consume tokens
    let result = mesh.router.consume_budget("test-agent", 100.0);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 900.0);
    
    // Check updated budget
    let remaining = mesh.router.get_remaining_budget("test-agent");
    assert_eq!(remaining, 900.0);
}

/// Test message routing
#[tokio::test]
async fn test_message_routing() {
    let mesh = AiMesh::new(AiMeshConfig::default()).unwrap();
    
    // Register endpoints
    mesh.router.register_endpoint(EndpointMetrics {
        endpoint_id: "cheap".into(),
        capacity: 100,
        current_load: 0,
        cost_per_1k_tokens: 1.0,
        latency_p99_ms: 100.0,
        error_rate: 0.0,
        last_health_check: 0,
        health_status: HealthStatus::Healthy as i32,
    });
    
    mesh.router.register_endpoint(EndpointMetrics {
        endpoint_id: "expensive".into(),
        capacity: 100,
        current_load: 0,
        cost_per_1k_tokens: 50.0,
        latency_p99_ms: 50.0,
        error_rate: 0.0,
        last_health_check: 0,
        health_status: HealthStatus::Healthy as i32,
    });
    
    // Route a message
    let msg = AiMessage::new(
        "test-agent".into(),
        b"Hello".to_vec(),
        100.0,
        i64::MAX,
    );
    
    let decision = mesh.router.route(&msg).await;
    assert!(decision.is_ok());
    
    // Should pick the cheaper endpoint
    let decision = decision.unwrap();
    assert_eq!(decision.target_endpoint, "cheap");
    assert_eq!(decision.fallback_endpoints.len(), 1);
}

/// Test deduplication
#[tokio::test]
async fn test_deduplication() {
    let mesh = AiMesh::new(AiMeshConfig::default()).unwrap();
    
    // Write a dedup entry
    mesh.storage.write_dedup("test-hash", b"cached".to_vec());
    
    // Check for duplicate
    let result = mesh.storage.check_dedup("test-hash");
    assert!(result.is_some());
    assert_eq!(result.unwrap(), b"cached".to_vec());
    
    // Non-existent hash
    let result = mesh.storage.check_dedup("nonexistent");
    assert!(result.is_none());
}

/// Test observability metrics
#[tokio::test]
async fn test_observability() {
    let mesh = AiMesh::new(AiMeshConfig::default()).unwrap();
    
    // Record some metrics
    mesh.observability.record_message("agent-1", true, 5.0, 100.0, 0.01);
    mesh.observability.record_message("agent-1", true, 3.0, 50.0, 0.005);
    mesh.observability.record_message("agent-2", false, 10.0, 0.0, 0.0);
    
    let stats = mesh.observability.get_stats();
    assert_eq!(stats.messages_total, 3);
    assert_eq!(stats.messages_success, 2);
    assert_eq!(stats.messages_failed, 1);
    assert_eq!(stats.tokens_consumed, 150);
}

/// Test message validation
#[tokio::test]
async fn test_message_validation() {
    // Valid message
    let valid = AiMessage::new(
        "valid-agent".into(),
        b"payload".to_vec(),
        100.0,
        i64::MAX,
    );
    assert!(valid.validate().is_ok());
    
    // Invalid agent ID (uppercase)
    let mut invalid = AiMessage::new(
        "INVALID".into(),
        b"payload".to_vec(),
        100.0,
        i64::MAX,
    );
    assert!(invalid.validate().is_err());
}

/// Test full message processing flow
#[tokio::test]
async fn test_full_message_flow() {
    let mesh = AiMesh::new(AiMeshConfig::default()).unwrap();
    
    // Register endpoint
    mesh.router.register_endpoint(EndpointMetrics {
        endpoint_id: "backend".into(),
        capacity: 100,
        current_load: 0,
        cost_per_1k_tokens: 10.0,
        latency_p99_ms: 100.0,
        error_rate: 0.0,
        last_health_check: 0,
        health_status: HealthStatus::Healthy as i32,
    });
    
    // Set budget
    mesh.router.set_budget("flow-agent", 1000.0, i64::MAX);
    
    // Process message
    let msg = AiMessage::new(
        "flow-agent".into(),
        b"Test message".to_vec(),
        100.0,
        i64::MAX,
    );
    
    let result = mesh.process_message(msg).await;
    assert!(result.is_ok());
    
    let ack = result.unwrap();
    assert!(ack.is_success());
    
    // Check stats
    let stats = mesh.get_stats();
    assert_eq!(stats.observability.messages_total, 1);
}
