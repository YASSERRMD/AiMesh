//! AiMesh Semantic Deduplication Module
//!
//! Blake3-based hashing for fast, secure deduplication of AI messages.

use blake3::Hasher;
use std::sync::Arc;
use dashmap::DashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::protocol::AiMessage;
use crate::storage::StorageLayer;

/// Semantic deduplicator with in-memory cache and storage backing
pub struct SemanticDeduplicator {
    /// In-memory cache: hash -> (timestamp_secs, result)
    cache: DashMap<String, (i64, Vec<u8>)>,
    /// Storage layer for persistent dedup
    storage: Option<Arc<StorageLayer>>,
    /// TTL for cached entries in seconds
    ttl_secs: u64,
}

impl SemanticDeduplicator {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            cache: DashMap::new(),
            storage: None,
            ttl_secs,
        }
    }
    
    pub fn with_storage(mut self, storage: Arc<StorageLayer>) -> Self {
        self.storage = Some(storage);
        self
    }
    
    /// Compute semantic hash of a message
    pub fn compute_hash(&self, message: &AiMessage) -> String {
        let mut hasher = Hasher::new();
        hasher.update(&message.payload);
        hasher.update(message.dedup_context.as_bytes());
        hex::encode(hasher.finalize().as_bytes())
    }
    
    /// Check if message is a duplicate, returns cached result if found
    pub fn check_duplicate(&self, message: &AiMessage) -> Option<Vec<u8>> {
        let hash = self.compute_hash(message);
        let now = Self::now_secs();
        
        // Check in-memory cache first
        if let Some(entry) = self.cache.get(&hash) {
            let (timestamp, result) = entry.value();
            if now - *timestamp < self.ttl_secs as i64 {
                return Some(result.clone());
            } else {
                drop(entry);
                self.cache.remove(&hash);
            }
        }
        
        // Check persistent storage
        if let Some(storage) = &self.storage {
            if let Some(result) = storage.check_dedup(&hash) {
                // Populate cache
                self.cache.insert(hash, (now, result.clone()));
                return Some(result);
            }
        }
        
        None
    }
    
    /// Record a message and its result for deduplication
    pub fn record(&self, message: &AiMessage, result: Vec<u8>) {
        let hash = self.compute_hash(message);
        let now = Self::now_secs();
        
        // Store in cache
        self.cache.insert(hash.clone(), (now, result.clone()));
        
        // Store persistently
        if let Some(storage) = &self.storage {
            storage.write_dedup(&hash, result);
        }
    }
    
    /// Cleanup expired entries
    pub fn cleanup(&self) -> usize {
        let now = Self::now_secs();
        let mut removed = 0;
        
        self.cache.retain(|_, (timestamp, _)| {
            let keep = now - *timestamp < self.ttl_secs as i64;
            if !keep { removed += 1; }
            keep
        });
        
        removed
    }
    
    /// Get cache size
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }
    
    fn now_secs() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compute_hash() {
        let dedup = SemanticDeduplicator::new(3600);
        
        let msg1 = AiMessage::new("agent-1".into(), b"Hello".to_vec(), 100.0, i64::MAX);
        let msg2 = AiMessage::new("agent-2".into(), b"Hello".to_vec(), 100.0, i64::MAX);
        
        // Same payload = same hash (agent_id doesn't affect hash)
        assert_eq!(dedup.compute_hash(&msg1), dedup.compute_hash(&msg2));
    }
    
    #[test]
    fn test_duplicate_detection() {
        let dedup = SemanticDeduplicator::new(3600);
        
        let msg = AiMessage::new("agent-1".into(), b"Test".to_vec(), 100.0, i64::MAX);
        
        // First time: no duplicate
        assert!(dedup.check_duplicate(&msg).is_none());
        
        // Record the result
        dedup.record(&msg, b"cached response".to_vec());
        
        // Second time: duplicate found
        let result = dedup.check_duplicate(&msg);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), b"cached response".to_vec());
    }
}
