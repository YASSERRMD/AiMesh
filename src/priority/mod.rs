//! AiMesh Priority Queue Module
//!
//! Multi-level priority queue with deadline awareness and fair scheduling.

use std::collections::BinaryHeap;
use std::cmp::Ordering;
use std::sync::Arc;
use parking_lot::Mutex;
use crossbeam_channel::{Sender, Receiver, unbounded};
use tracing::{debug, warn};

use crate::protocol::AiMessage;

/// Priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PriorityLevel {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl From<i32> for PriorityLevel {
    fn from(priority: i32) -> Self {
        match priority {
            0..=25 => PriorityLevel::Low,
            26..=50 => PriorityLevel::Normal,
            51..=75 => PriorityLevel::High,
            _ => PriorityLevel::Critical,
        }
    }
}

/// Prioritized message wrapper
#[derive(Debug, Clone)]
pub struct PrioritizedMessage {
    pub message: AiMessage,
    pub priority_level: PriorityLevel,
    pub enqueued_at: i64,
    pub deadline_ms: i64,
}

impl PrioritizedMessage {
    pub fn new(message: AiMessage) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64;
        
        let priority_level = PriorityLevel::from(message.priority);
        let deadline_ms = message.deadline_ms;
        
        Self {
            message,
            priority_level,
            enqueued_at: now,
            deadline_ms,
        }
    }
    
    /// Time until deadline in milliseconds
    pub fn time_until_deadline(&self) -> i64 {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        
        self.deadline_ms - now_ms
    }
    
    /// Check if deadline has passed
    pub fn is_expired(&self) -> bool {
        if self.deadline_ms == i64::MAX {
            return false;
        }
        self.time_until_deadline() < 0
    }
    
    /// Calculate effective priority (higher = more urgent)
    pub fn effective_priority(&self) -> i64 {
        let base_priority = (self.priority_level as i64) * 1000;
        
        // Boost priority as deadline approaches
        let deadline_boost = if self.deadline_ms == i64::MAX {
            0
        } else {
            let time_left = self.time_until_deadline();
            if time_left < 1000 { // Less than 1 second
                500
            } else if time_left < 5000 { // Less than 5 seconds
                200
            } else if time_left < 10000 { // Less than 10 seconds
                100
            } else {
                0
            }
        };
        
        base_priority + deadline_boost
    }
}

impl Eq for PrioritizedMessage {}

impl PartialEq for PrioritizedMessage {
    fn eq(&self, other: &Self) -> bool {
        self.message.message_id == other.message.message_id
    }
}

impl Ord for PrioritizedMessage {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher effective priority = should be processed first
        self.effective_priority().cmp(&other.effective_priority())
    }
}

impl PartialOrd for PrioritizedMessage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Multi-level priority queue
pub struct PriorityQueue {
    /// Max heap for priority ordering
    heap: Mutex<BinaryHeap<PrioritizedMessage>>,
    /// Channel for async notification
    notify_tx: Sender<()>,
    notify_rx: Receiver<()>,
    /// Configuration
    config: PriorityQueueConfig,
}

/// Priority queue configuration
#[derive(Debug, Clone)]
pub struct PriorityQueueConfig {
    /// Maximum queue size
    pub max_size: usize,
    /// Enable deadline-aware scheduling
    pub deadline_aware: bool,
    /// Drop expired messages
    pub drop_expired: bool,
}

impl Default for PriorityQueueConfig {
    fn default() -> Self {
        Self {
            max_size: 100_000,
            deadline_aware: true,
            drop_expired: true,
        }
    }
}

impl PriorityQueue {
    pub fn new(config: PriorityQueueConfig) -> Self {
        let (notify_tx, notify_rx) = unbounded();
        
        Self {
            heap: Mutex::new(BinaryHeap::with_capacity(config.max_size)),
            notify_tx,
            notify_rx,
            config,
        }
    }
    
    /// Enqueue a message
    pub fn push(&self, message: AiMessage) -> Result<(), QueueError> {
        let mut heap = self.heap.lock();
        
        if heap.len() >= self.config.max_size {
            return Err(QueueError::Full);
        }
        
        let prioritized = PrioritizedMessage::new(message);
        debug!(
            message_id = %prioritized.message.message_id,
            priority = ?prioritized.priority_level,
            "Enqueued message"
        );
        
        heap.push(prioritized);
        let _ = self.notify_tx.send(());
        
        Ok(())
    }
    
    /// Dequeue the highest priority message
    pub fn pop(&self) -> Option<PrioritizedMessage> {
        let mut heap = self.heap.lock();
        
        if self.config.drop_expired {
            // Remove expired messages
            while let Some(msg) = heap.peek() {
                if msg.is_expired() {
                    let expired = heap.pop();
                    if let Some(e) = expired {
                        warn!(
                            message_id = %e.message.message_id,
                            "Dropped expired message"
                        );
                    }
                } else {
                    break;
                }
            }
        }
        
        heap.pop()
    }
    
    /// Peek at the highest priority message without removing
    pub fn peek(&self) -> Option<PrioritizedMessage> {
        self.heap.lock().peek().cloned()
    }
    
    /// Get queue length
    pub fn len(&self) -> usize {
        self.heap.lock().len()
    }
    
    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.heap.lock().is_empty()
    }
    
    /// Wait for a message (blocking)
    pub fn wait(&self) -> Option<PrioritizedMessage> {
        loop {
            if let Some(msg) = self.pop() {
                return Some(msg);
            }
            
            // Wait for notification
            if self.notify_rx.recv().is_err() {
                return None;
            }
        }
    }
    
    /// Get queue statistics
    pub fn stats(&self) -> QueueStats {
        let heap = self.heap.lock();
        
        let mut by_priority = [0usize; 4];
        let mut expired = 0;
        
        for msg in heap.iter() {
            by_priority[msg.priority_level as usize] += 1;
            if msg.is_expired() {
                expired += 1;
            }
        }
        
        QueueStats {
            total: heap.len(),
            critical: by_priority[3],
            high: by_priority[2],
            normal: by_priority[1],
            low: by_priority[0],
            expired,
        }
    }
    
    /// Clear the queue
    pub fn clear(&self) {
        self.heap.lock().clear();
    }
}

/// Queue error
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("Queue is full")]
    Full,
    #[error("Queue is closed")]
    Closed,
}

/// Queue statistics
#[derive(Debug, Clone)]
pub struct QueueStats {
    pub total: usize,
    pub critical: usize,
    pub high: usize,
    pub normal: usize,
    pub low: usize,
    pub expired: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_priority_ordering() {
        let queue = PriorityQueue::new(PriorityQueueConfig::default());
        
        // Enqueue messages with different priorities
        let mut low = AiMessage::new("agent".into(), b"low".to_vec(), 100.0, i64::MAX);
        low.priority = 10;
        
        let mut high = AiMessage::new("agent".into(), b"high".to_vec(), 100.0, i64::MAX);
        high.priority = 80;
        
        let mut normal = AiMessage::new("agent".into(), b"normal".to_vec(), 100.0, i64::MAX);
        normal.priority = 50;
        
        queue.push(low).unwrap();
        queue.push(high.clone()).unwrap();
        queue.push(normal).unwrap();
        
        // Should get high priority first
        let first = queue.pop().unwrap();
        assert_eq!(first.message.payload, b"high");
        assert_eq!(first.priority_level, PriorityLevel::Critical);
    }
    
    #[test]
    fn test_deadline_boost() {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        
        let mut urgent = AiMessage::new("agent".into(), b"urgent".to_vec(), 100.0, now_ms + 500);
        urgent.priority = 50;
        
        let mut normal = AiMessage::new("agent".into(), b"normal".to_vec(), 100.0, i64::MAX);
        normal.priority = 50;
        
        let urgent_pm = PrioritizedMessage::new(urgent);
        let normal_pm = PrioritizedMessage::new(normal);
        
        // Urgent should have higher effective priority due to deadline
        assert!(urgent_pm.effective_priority() > normal_pm.effective_priority());
    }
    
    #[test]
    fn test_queue_stats() {
        let queue = PriorityQueue::new(PriorityQueueConfig::default());
        
        for i in 0..10 {
            let mut msg = AiMessage::new("agent".into(), vec![i], 100.0, i64::MAX);
            msg.priority = (i * 10) as i32;
            queue.push(msg).unwrap();
        }
        
        let stats = queue.stats();
        assert_eq!(stats.total, 10);
    }
}
