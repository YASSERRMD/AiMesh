//! AiMesh Orchestration Module
//!
//! Scatter-gather task orchestration with dependency resolution.

use std::collections::HashMap;
use std::sync::Arc;
use dashmap::DashMap;
use thiserror::Error;
use tracing::{debug, info, warn};

use crate::protocol::{AiMessage, TaskState, TaskStep, TaskStatus};
use crate::routing::CostAwareRouter;
use crate::storage::StorageLayer;

#[derive(Error, Debug)]
pub enum OrchestrationError {
    #[error("Task not found: {0}")]
    TaskNotFound(String),
    #[error("Dependency not satisfied: {0}")]
    DependencyNotSatisfied(String),
    #[error("Task timeout: {0}")]
    Timeout(String),
    #[error("Task failed: {0}")]
    TaskFailed(String),
    #[error("Storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
}

/// Task dependency graph
#[derive(Debug, Clone)]
pub struct TaskGraph {
    pub task_id: String,
    pub steps: Vec<TaskStepDef>,
}

#[derive(Debug, Clone)]
pub struct TaskStepDef {
    pub step_id: String,
    pub message: AiMessage,
    pub dependencies: Vec<String>,
}

/// Orchestration engine for scatter-gather workflows
pub struct OrchestrationEngine {
    tasks: DashMap<String, TaskState>,
    storage: Arc<StorageLayer>,
    router: Arc<CostAwareRouter>,
}

impl OrchestrationEngine {
    pub fn new(storage: Arc<StorageLayer>, router: Arc<CostAwareRouter>) -> Self {
        Self {
            tasks: DashMap::new(),
            storage,
            router,
        }
    }
    
    /// Begin a new task from a dependency graph
    pub async fn begin_task(&self, graph: TaskGraph) -> Result<String, OrchestrationError> {
        let task_id = graph.task_id.clone();
        
        let steps: Vec<TaskStep> = graph.steps.iter().map(|def| {
            TaskStep {
                step_id: def.step_id.clone(),
                status: TaskStatus::TaskPending as i32,
                dependencies: def.dependencies.clone(),
                message: Some(def.message.clone()),
                result: Vec::new(),
                error: String::new(),
            }
        }).collect();
        
        let state = TaskState {
            task_id: task_id.clone(),
            status: TaskStatus::TaskPending as i32,
            steps,
            started_at: Self::now_ns(),
            completed_at: 0,
            results: HashMap::new(),
            error: String::new(),
        };
        
        self.tasks.insert(task_id.clone(), state.clone());
        self.storage.write_task_state(&task_id, &state).await?;
        
        info!(task_id = %task_id, steps = graph.steps.len(), "Started task");
        Ok(task_id)
    }
    
    /// Execute the next ready step(s) in a task
    pub async fn execute_next(&self, task_id: &str) -> Result<Vec<String>, OrchestrationError> {
        let mut task = self.tasks
            .get_mut(task_id)
            .ok_or_else(|| OrchestrationError::TaskNotFound(task_id.into()))?;
        
        // Find steps that are ready (pending with all deps satisfied)
        let ready_steps: Vec<usize> = task.steps.iter().enumerate()
            .filter(|(_, step)| {
                step.status == TaskStatus::TaskPending as i32 &&
                step.dependencies.iter().all(|dep| {
                    task.steps.iter().any(|s| s.step_id == *dep && s.status == TaskStatus::TaskCompleted as i32)
                })
            })
            .map(|(i, _)| i)
            .collect();
        
        let mut executed = Vec::new();
        
        for idx in ready_steps {
            let step_id = task.steps[idx].step_id.clone();
            task.steps[idx].status = TaskStatus::TaskRunning as i32;
            
            // TODO: Actually route and execute the message
            // For now, mark as completed with empty result
            task.steps[idx].status = TaskStatus::TaskCompleted as i32;
            task.steps[idx].result = Vec::new();
            
            executed.push(step_id);
        }
        
        // Check if all steps are complete
        let all_complete = task.steps.iter().all(|s| s.status == TaskStatus::TaskCompleted as i32);
        if all_complete {
            task.status = TaskStatus::TaskCompleted as i32;
            task.completed_at = Self::now_ns();
            info!(task_id = %task_id, "Task completed");
        }
        
        Ok(executed)
    }
    
    /// Wait for task completion with timeout
    pub async fn wait_for_completion(
        &self,
        task_id: &str,
        timeout_ms: u64,
    ) -> Result<TaskState, OrchestrationError> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);
        
        loop {
            if let Some(task) = self.tasks.get(task_id) {
                if task.status == TaskStatus::TaskCompleted as i32 ||
                   task.status == TaskStatus::TaskFailed as i32 {
                    return Ok(task.clone());
                }
            } else {
                return Err(OrchestrationError::TaskNotFound(task_id.into()));
            }
            
            if start.elapsed() > timeout {
                return Err(OrchestrationError::Timeout(task_id.into()));
            }
            
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
    
    /// Get task state
    pub fn get_task(&self, task_id: &str) -> Option<TaskState> {
        self.tasks.get(task_id).map(|t| t.clone())
    }
    
    fn now_ns() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::RouterConfig;
    use crate::storage::StorageConfig;
    
    #[tokio::test]
    async fn test_simple_task() {
        let storage = Arc::new(StorageLayer::new(StorageConfig::default()).unwrap());
        let router = Arc::new(CostAwareRouter::new(RouterConfig::default()));
        let engine = OrchestrationEngine::new(storage, router);
        
        let msg = AiMessage::new("test-agent".into(), b"test".to_vec(), 100.0, i64::MAX);
        
        let graph = TaskGraph {
            task_id: "task-1".into(),
            steps: vec![
                TaskStepDef {
                    step_id: "step-1".into(),
                    message: msg.clone(),
                    dependencies: vec![],
                },
                TaskStepDef {
                    step_id: "step-2".into(),
                    message: msg,
                    dependencies: vec!["step-1".into()],
                },
            ],
        };
        
        let task_id = engine.begin_task(graph).await.unwrap();
        assert_eq!(task_id, "task-1");
        
        // Execute first step
        let executed = engine.execute_next(&task_id).await.unwrap();
        assert_eq!(executed, vec!["step-1"]);
        
        // Execute second step
        let executed = engine.execute_next(&task_id).await.unwrap();
        assert_eq!(executed, vec!["step-2"]);
        
        // Task should be complete
        let task = engine.get_task(&task_id).unwrap();
        assert_eq!(task.status, TaskStatus::TaskCompleted as i32);
    }
}
