//! AiMesh main binary

use aimesh::{AiMesh, AiMeshConfig, AiMessage, StorageConfig};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    
    info!("Starting AiMesh v{}", env!("CARGO_PKG_VERSION"));
    
    // Configure AiMesh with Barq backends
    // Barq-DB: localhost:8080 (vector database)
    // Barq-GraphDB: localhost:8081 (graph database)
    let config = AiMeshConfig {
        bind_addr: "0.0.0.0:9000".into(), // AiMesh on port 9000
        storage: StorageConfig {
            barq_db_url: "http://localhost:8080".into(),
            barq_graphdb_url: "http://localhost:8081".into(),
            messages_collection: "aimesh_messages".into(),
            dedup_collection: "aimesh_dedup".into(),
            embedding_dim: 384,
            dedup_ttl_secs: 3600,
        },
        ..Default::default()
    };
    
    // Create AiMesh instance
    let mesh = AiMesh::new(config)?;
    
    // Initialize storage (create collections)
    if let Err(e) = mesh.storage.initialize().await {
        info!("Note: Could not initialize storage: {} (will use cache)", e);
    }
    
    // Check storage health
    match mesh.storage.health_check().await {
        Ok(true) => info!("Connected to Barq backends successfully"),
        Ok(false) => info!("Warning: Barq health check failed, using local cache"),
        Err(e) => info!("Warning: Could not connect to Barq: {}", e),
    }
    
    // Register AI model endpoints
    mesh.router.register_endpoint(aimesh::EndpointMetrics {
        endpoint_id: "openai-gpt4".into(),
        capacity: 1000,
        current_load: 0,
        cost_per_1k_tokens: 30.0,
        latency_p99_ms: 500.0,
        error_rate: 0.01,
        last_health_check: 0,
        health_status: aimesh::HealthStatus::Healthy as i32,
    });
    
    mesh.router.register_endpoint(aimesh::EndpointMetrics {
        endpoint_id: "anthropic-claude".into(),
        capacity: 1000,
        current_load: 0,
        cost_per_1k_tokens: 15.0,
        latency_p99_ms: 300.0,
        error_rate: 0.005,
        last_health_check: 0,
        health_status: aimesh::HealthStatus::Healthy as i32,
    });
    
    mesh.router.register_endpoint(aimesh::EndpointMetrics {
        endpoint_id: "local-llama".into(),
        capacity: 100,
        current_load: 0,
        cost_per_1k_tokens: 0.1,
        latency_p99_ms: 100.0,
        error_rate: 0.02,
        last_health_check: 0,
        health_status: aimesh::HealthStatus::Healthy as i32,
    });
    
    info!("Registered 3 AI model endpoints");
    
    // Set budget for demo agent
    mesh.router.set_budget("demo-agent", 10000.0, i64::MAX);
    
    // Process a test message
    let test_msg = AiMessage::new(
        "demo-agent".into(),
        b"Hello, AiMesh! Route me to the best model.".to_vec(),
        1000.0,
        i64::MAX,
    );
    
    match mesh.process_message(test_msg).await {
        Ok(ack) => {
            info!(
                message_id = %ack.original_message_id,
                latency_ms = ack.processing_latency_ms,
                tokens = ack.tokens_used,
                "Processed test message successfully"
            );
        }
        Err(e) => {
            info!("Error processing message: {}", e);
        }
    }
    
    // Print system stats
    let stats = mesh.get_stats();
    info!(
        messages = stats.observability.messages_total,
        success = stats.observability.messages_success,
        throughput = format!("{:.2}/sec", stats.observability.throughput_per_sec),
        endpoints = stats.router.endpoints_count,
        "System statistics"
    );
    
    info!("AiMesh ready on {}", mesh.config.bind_addr);
    info!("Using Barq-DB at http://localhost:8080");
    info!("Using Barq-GraphDB at http://localhost:8081");
    
    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    
    info!("Shutting down AiMesh");
    Ok(())
}
