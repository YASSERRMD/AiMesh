//! AiMesh Storage Module
//!
//! Dual-backend storage using Barq ecosystem:
//! - Barq-DB: Vector database for messages and semantic dedup
//! - Barq-GraphDB: Graph database for agent relationships

use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tracing::{debug, info, warn};
use dashmap::DashMap;

use crate::protocol::{AiMessage, TaskState, BudgetInfo};

/// Storage errors
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Barq-DB error: {0}")]
    BarqDbError(String),
    #[error("Barq-GraphDB error: {0}")]
    BarqGraphError(String),
    #[error("Connection error: {0}")]
    ConnectionError(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Not found: {0}")]
    NotFound(String),
}

/// Storage configuration
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Barq-DB HTTP endpoint (vector database)
    pub barq_db_url: String,
    /// Barq-GraphDB HTTP endpoint (graph database)
    pub barq_graphdb_url: String,
    /// Collection name for messages
    pub messages_collection: String,
    /// Collection name for dedup cache
    pub dedup_collection: String,
    /// Embedding dimension
    pub embedding_dim: u32,
    /// Dedup TTL in seconds
    pub dedup_ttl_secs: u64,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            barq_db_url: "http://localhost:8080".into(),  // barq-db default
            barq_graphdb_url: "http://localhost:8081".into(), // barq-graphdb default
            messages_collection: "aimesh_messages".into(),
            dedup_collection: "aimesh_dedup".into(),
            embedding_dim: 384, // Common embedding size
            dedup_ttl_secs: 3600,
        }
    }
}

/// Barq-DB client for vector storage
pub struct BarqDbClient {
    http_client: reqwest::Client,
    base_url: String,
}

impl BarqDbClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            base_url: base_url.to_string(),
        }
    }

    pub async fn health_check(&self) -> Result<bool, StorageError> {
        let resp = self.http_client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map_err(|e| StorageError::ConnectionError(e.to_string()))?;
        Ok(resp.status().is_success())
    }

    pub async fn create_collection(&self, name: &str, dimension: u32, metric: &str) -> Result<(), StorageError> {
        let body = serde_json::json!({
            "name": name,
            "dimension": dimension,
            "metric": metric
        });
        
        self.http_client
            .post(format!("{}/collections", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| StorageError::BarqDbError(e.to_string()))?;
        Ok(())
    }

    pub async fn insert_document(&self, collection: &str, id: &str, vector: Vec<f32>, payload: serde_json::Value) -> Result<(), StorageError> {
        let body = serde_json::json!({
            "collection": collection,
            "id": id,
            "vector": vector,
            "payload_json": payload.to_string()
        });
        
        self.http_client
            .post(format!("{}/documents", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| StorageError::BarqDbError(e.to_string()))?;
        Ok(())
    }

    pub async fn search(&self, collection: &str, vector: Vec<f32>, top_k: u32) -> Result<Vec<SearchResult>, StorageError> {
        let body = serde_json::json!({
            "collection": collection,
            "vector": vector,
            "top_k": top_k
        });
        
        let resp = self.http_client
            .post(format!("{}/search", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| StorageError::BarqDbError(e.to_string()))?;
        
        let response: SearchResponse = resp.json()
            .await
            .map_err(|e| StorageError::BarqDbError(e.to_string()))?;
        
        Ok(response.results)
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub payload_json: String,
}

#[derive(Debug, serde::Deserialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
}

/// Barq-GraphDB client for graph relationships
pub struct BarqGraphClient {
    http_client: reqwest::Client,
    base_url: String,
}

impl BarqGraphClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            base_url: base_url.to_string(),
        }
    }

    pub async fn health_check(&self) -> Result<bool, StorageError> {
        let resp = self.http_client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map_err(|e| StorageError::ConnectionError(e.to_string()))?;
        Ok(resp.status().is_success())
    }

    pub async fn create_node(&self, id: u64, label: &str) -> Result<(), StorageError> {
        let body = serde_json::json!({
            "id": id,
            "label": label
        });
        
        self.http_client
            .post(format!("{}/nodes", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| StorageError::BarqGraphError(e.to_string()))?;
        Ok(())
    }

    pub async fn create_edge(&self, from: u64, to: u64, edge_type: &str) -> Result<(), StorageError> {
        let body = serde_json::json!({
            "from": from,
            "to": to,
            "type": edge_type
        });
        
        self.http_client
            .post(format!("{}/edges", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| StorageError::BarqGraphError(e.to_string()))?;
        Ok(())
    }
}

/// Storage layer using Barq-DB and Barq-GraphDB
pub struct StorageLayer {
    config: StorageConfig,
    barq_db: BarqDbClient,
    barq_graph: BarqGraphClient,
    // In-memory caches for fast access
    message_cache: DashMap<String, AiMessage>,
    task_cache: DashMap<String, TaskState>,
    dedup_cache: DashMap<String, (i64, Vec<u8>)>,
    budget_cache: DashMap<String, BudgetInfo>,
}

impl StorageLayer {
    pub fn new(config: StorageConfig) -> Result<Self, StorageError> {
        let barq_db = BarqDbClient::new(&config.barq_db_url);
        let barq_graph = BarqGraphClient::new(&config.barq_graphdb_url);
        
        info!(
            barq_db = %config.barq_db_url,
            barq_graph = %config.barq_graphdb_url,
            "Initialized storage layer with Barq backends"
        );
        
        Ok(Self {
            config,
            barq_db,
            barq_graph,
            message_cache: DashMap::new(),
            task_cache: DashMap::new(),
            dedup_cache: DashMap::new(),
            budget_cache: DashMap::new(),
        })
    }

    /// Initialize collections in Barq-DB
    pub async fn initialize(&self) -> Result<(), StorageError> {
        // Create messages collection
        self.barq_db.create_collection(
            &self.config.messages_collection,
            self.config.embedding_dim,
            "Cosine",
        ).await.ok(); // Ignore if exists
        
        // Create dedup collection
        self.barq_db.create_collection(
            &self.config.dedup_collection,
            self.config.embedding_dim,
            "Cosine",
        ).await.ok();
        
        info!("Initialized Barq-DB collections");
        Ok(())
    }

    pub async fn health_check(&self) -> Result<bool, StorageError> {
        let db_ok = self.barq_db.health_check().await.unwrap_or(false);
        let graph_ok = self.barq_graph.health_check().await.unwrap_or(false);
        Ok(db_ok && graph_ok)
    }

    /// Write a message to storage
    pub async fn write_message(&self, message: &AiMessage) -> Result<(), StorageError> {
        // Cache locally
        self.message_cache.insert(message.message_id.clone(), message.clone());
        
        // Store in Barq-DB with a simple embedding (hash-based for now)
        let embedding = self.payload_to_embedding(&message.payload);
        let payload = serde_json::json!({
            "agent_id": message.agent_id,
            "timestamp": message.timestamp,
            "budget_tokens": message.budget_tokens,
            "priority": message.priority,
        });
        
        self.barq_db.insert_document(
            &self.config.messages_collection,
            &message.message_id,
            embedding,
            payload,
        ).await?;
        
        // Create graph node for the message
        let node_id = hash_to_u64(&message.message_id);
        self.barq_graph.create_node(node_id, &format!("msg:{}", message.agent_id)).await.ok();
        
        debug!(message_id = %message.message_id, "Wrote message to Barq-DB");
        Ok(())
    }

    /// Read a message (from cache)
    pub fn read_message(&self, message_id: &str) -> Option<AiMessage> {
        self.message_cache.get(message_id).map(|r| r.clone())
    }

    /// Write task state
    pub async fn write_task_state(&self, task_id: &str, state: &TaskState) -> Result<(), StorageError> {
        self.task_cache.insert(task_id.to_string(), state.clone());
        
        // Create graph relationships
        let task_node_id = hash_to_u64(task_id);
        self.barq_graph.create_node(task_node_id, &format!("task:{}", task_id)).await.ok();
        
        for step in &state.steps {
            let step_node_id = hash_to_u64(&step.step_id);
            self.barq_graph.create_node(step_node_id, &format!("step:{}", step.step_id)).await.ok();
            self.barq_graph.create_edge(task_node_id, step_node_id, "has_step").await.ok();
        }
        
        Ok(())
    }

    pub fn read_task_state(&self, task_id: &str) -> Option<TaskState> {
        self.task_cache.get(task_id).map(|r| r.clone())
    }

    /// Write dedup record
    pub fn write_dedup(&self, hash: &str, result: Vec<u8>) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        self.dedup_cache.insert(hash.to_string(), (now, result));
    }

    /// Check for duplicate
    pub fn check_dedup(&self, hash: &str) -> Option<Vec<u8>> {
        if let Some(entry) = self.dedup_cache.get(hash) {
            let (timestamp, result) = entry.value();
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
            if now - timestamp < self.config.dedup_ttl_secs as i64 {
                return Some(result.clone());
            } else {
                drop(entry);
                self.dedup_cache.remove(hash);
            }
        }
        None
    }

    /// Semantic search for similar messages
    pub async fn semantic_search(&self, embedding: Vec<f32>, top_k: u32) -> Result<Vec<SearchResult>, StorageError> {
        self.barq_db.search(&self.config.messages_collection, embedding, top_k).await
    }

    /// Write budget
    pub fn write_budget(&self, budget: &BudgetInfo) {
        self.budget_cache.insert(budget.agent_id.clone(), budget.clone());
    }

    pub fn read_budget(&self, agent_id: &str) -> Option<BudgetInfo> {
        self.budget_cache.get(agent_id).map(|r| r.clone())
    }

    /// Link agents in the graph
    pub async fn link_agents(&self, from: &str, to: &str, relation: &str) -> Result<(), StorageError> {
        self.barq_graph.create_edge(hash_to_u64(from), hash_to_u64(to), relation).await
    }

    /// Convert payload to embedding (simple hash-based for now)
    fn payload_to_embedding(&self, payload: &[u8]) -> Vec<f32> {
        let hash = blake3::hash(payload);
        let bytes = hash.as_bytes();
        // Expand hash to embedding dimension
        let mut embedding = Vec::with_capacity(self.config.embedding_dim as usize);
        for i in 0..self.config.embedding_dim as usize {
            let byte = bytes[i % 32];
            embedding.push((byte as f32 / 255.0) - 0.5);
        }
        embedding
    }

    /// Cleanup expired dedup entries
    pub fn cleanup_expired(&self) -> usize {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let ttl = self.config.dedup_ttl_secs as i64;
        let mut removed = 0;
        self.dedup_cache.retain(|_, (ts, _)| {
            let keep = now - *ts < ttl;
            if !keep { removed += 1; }
            keep
        });
        removed
    }
}

fn hash_to_u64(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

// Re-export StorageBackend for compatibility
#[derive(Debug, Clone, PartialEq)]
pub enum StorageBackend {
    BarqDB,
    BarqGraphDB,
    Hybrid,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dedup_cache() {
        let config = StorageConfig::default();
        let storage = StorageLayer::new(config).unwrap();
        
        storage.write_dedup("hash123", b"result".to_vec());
        let result = storage.check_dedup("hash123");
        assert!(result.is_some());
    }
    
    #[test]
    fn test_payload_embedding() {
        let config = StorageConfig::default();
        let storage = StorageLayer::new(config).unwrap();
        
        let embedding = storage.payload_to_embedding(b"test payload");
        assert_eq!(embedding.len(), 384);
    }
}
