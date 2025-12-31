//! AiMesh Multi-tenancy Module
//!
//! Tenant isolation, quota management, and namespace separation.

use std::collections::HashMap;
use dashmap::DashMap;
use thiserror::Error;
use tracing::{info, debug};

#[derive(Error, Debug)]
pub enum TenantError {
    #[error("Tenant not found: {0}")]
    NotFound(String),
    #[error("Tenant quota exceeded: {0}")]
    QuotaExceeded(String),
    #[error("Tenant suspended: {0}")]
    Suspended(String),
    #[error("Invalid tenant configuration: {0}")]
    InvalidConfig(String),
}

/// Tenant status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantStatus {
    Active,
    Suspended,
    PendingDeletion,
}

impl Default for TenantStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// Tenant tier for resource allocation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantTier {
    Free,
    Starter,
    Professional,
    Enterprise,
}

impl TenantTier {
    /// Get default quotas for tier
    pub fn default_quotas(&self) -> TenantQuotas {
        match self {
            TenantTier::Free => TenantQuotas {
                max_agents: 5,
                max_messages_per_day: 1_000,
                max_tokens_per_day: 10_000,
                max_endpoints: 2,
                max_concurrent_requests: 10,
                storage_bytes: 100 * 1024 * 1024, // 100MB
            },
            TenantTier::Starter => TenantQuotas {
                max_agents: 25,
                max_messages_per_day: 50_000,
                max_tokens_per_day: 500_000,
                max_endpoints: 10,
                max_concurrent_requests: 100,
                storage_bytes: 1024 * 1024 * 1024, // 1GB
            },
            TenantTier::Professional => TenantQuotas {
                max_agents: 100,
                max_messages_per_day: 500_000,
                max_tokens_per_day: 5_000_000,
                max_endpoints: 50,
                max_concurrent_requests: 500,
                storage_bytes: 10 * 1024 * 1024 * 1024, // 10GB
            },
            TenantTier::Enterprise => TenantQuotas {
                max_agents: u64::MAX,
                max_messages_per_day: u64::MAX,
                max_tokens_per_day: u64::MAX,
                max_endpoints: u64::MAX as u32,
                max_concurrent_requests: u64::MAX as u32,
                storage_bytes: u64::MAX,
            },
        }
    }
}

impl Default for TenantTier {
    fn default() -> Self {
        Self::Free
    }
}

/// Tenant resource quotas
#[derive(Debug, Clone)]
pub struct TenantQuotas {
    pub max_agents: u64,
    pub max_messages_per_day: u64,
    pub max_tokens_per_day: u64,
    pub max_endpoints: u32,
    pub max_concurrent_requests: u32,
    pub storage_bytes: u64,
}

/// Tenant usage tracking
#[derive(Debug, Clone, Default)]
pub struct TenantUsage {
    pub agents_count: u64,
    pub messages_today: u64,
    pub tokens_today: u64,
    pub endpoints_count: u32,
    pub concurrent_requests: u32,
    pub storage_used: u64,
    pub last_reset: i64,
}

impl TenantUsage {
    /// Check if quota exceeded
    pub fn check_quota(&self, quotas: &TenantQuotas) -> Result<(), TenantError> {
        if self.agents_count >= quotas.max_agents {
            return Err(TenantError::QuotaExceeded("max_agents".into()));
        }
        if self.messages_today >= quotas.max_messages_per_day {
            return Err(TenantError::QuotaExceeded("max_messages_per_day".into()));
        }
        if self.tokens_today >= quotas.max_tokens_per_day {
            return Err(TenantError::QuotaExceeded("max_tokens_per_day".into()));
        }
        if self.endpoints_count >= quotas.max_endpoints {
            return Err(TenantError::QuotaExceeded("max_endpoints".into()));
        }
        if self.concurrent_requests >= quotas.max_concurrent_requests {
            return Err(TenantError::QuotaExceeded("max_concurrent_requests".into()));
        }
        if self.storage_used >= quotas.storage_bytes {
            return Err(TenantError::QuotaExceeded("storage_bytes".into()));
        }
        Ok(())
    }
    
    /// Get utilization percentages
    pub fn utilization(&self, quotas: &TenantQuotas) -> HashMap<String, f64> {
        let mut util = HashMap::new();
        util.insert("agents".into(), self.agents_count as f64 / quotas.max_agents as f64 * 100.0);
        util.insert("messages".into(), self.messages_today as f64 / quotas.max_messages_per_day as f64 * 100.0);
        util.insert("tokens".into(), self.tokens_today as f64 / quotas.max_tokens_per_day as f64 * 100.0);
        util.insert("storage".into(), self.storage_used as f64 / quotas.storage_bytes as f64 * 100.0);
        util
    }
}

/// Tenant configuration
#[derive(Debug, Clone)]
pub struct Tenant {
    pub id: String,
    pub name: String,
    pub tier: TenantTier,
    pub status: TenantStatus,
    pub quotas: TenantQuotas,
    pub metadata: HashMap<String, String>,
    pub created_at: i64,
}

impl Tenant {
    pub fn new(id: String, name: String, tier: TenantTier) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64;
        
        Self {
            id,
            name,
            tier,
            status: TenantStatus::Active,
            quotas: tier.default_quotas(),
            metadata: HashMap::new(),
            created_at: now,
        }
    }
    
    pub fn is_active(&self) -> bool {
        self.status == TenantStatus::Active
    }
}

/// Multi-tenant manager
pub struct TenantManager {
    tenants: DashMap<String, Tenant>,
    usage: DashMap<String, TenantUsage>,
    /// Agent to tenant mapping
    agent_tenants: DashMap<String, String>,
}

impl TenantManager {
    pub fn new() -> Self {
        Self {
            tenants: DashMap::new(),
            usage: DashMap::new(),
            agent_tenants: DashMap::new(),
        }
    }
    
    /// Create a new tenant
    pub fn create_tenant(&self, id: String, name: String, tier: TenantTier) -> Result<Tenant, TenantError> {
        let tenant = Tenant::new(id.clone(), name, tier);
        self.tenants.insert(id.clone(), tenant.clone());
        self.usage.insert(id, TenantUsage::default());
        info!(tenant_id = %tenant.id, tier = ?tier, "Created tenant");
        Ok(tenant)
    }
    
    /// Get tenant by ID
    pub fn get_tenant(&self, tenant_id: &str) -> Option<Tenant> {
        self.tenants.get(tenant_id).map(|t| t.clone())
    }
    
    /// Update tenant tier
    pub fn update_tier(&self, tenant_id: &str, tier: TenantTier) -> Result<(), TenantError> {
        if let Some(mut tenant) = self.tenants.get_mut(tenant_id) {
            tenant.tier = tier;
            tenant.quotas = tier.default_quotas();
            info!(tenant_id = %tenant_id, tier = ?tier, "Updated tenant tier");
            Ok(())
        } else {
            Err(TenantError::NotFound(tenant_id.to_string()))
        }
    }
    
    /// Suspend a tenant
    pub fn suspend_tenant(&self, tenant_id: &str) -> Result<(), TenantError> {
        if let Some(mut tenant) = self.tenants.get_mut(tenant_id) {
            tenant.status = TenantStatus::Suspended;
            info!(tenant_id = %tenant_id, "Suspended tenant");
            Ok(())
        } else {
            Err(TenantError::NotFound(tenant_id.to_string()))
        }
    }
    
    /// Activate a tenant
    pub fn activate_tenant(&self, tenant_id: &str) -> Result<(), TenantError> {
        if let Some(mut tenant) = self.tenants.get_mut(tenant_id) {
            tenant.status = TenantStatus::Active;
            info!(tenant_id = %tenant_id, "Activated tenant");
            Ok(())
        } else {
            Err(TenantError::NotFound(tenant_id.to_string()))
        }
    }
    
    /// Register an agent to a tenant
    pub fn register_agent(&self, agent_id: &str, tenant_id: &str) -> Result<(), TenantError> {
        // Verify tenant exists and is active
        let tenant = self.tenants.get(tenant_id)
            .ok_or_else(|| TenantError::NotFound(tenant_id.to_string()))?;
        
        if !tenant.is_active() {
            return Err(TenantError::Suspended(tenant_id.to_string()));
        }
        
        // Check quota
        if let Some(mut usage) = self.usage.get_mut(tenant_id) {
            usage.check_quota(&tenant.quotas)?;
            usage.agents_count += 1;
        }
        
        self.agent_tenants.insert(agent_id.to_string(), tenant_id.to_string());
        debug!(agent_id = %agent_id, tenant_id = %tenant_id, "Registered agent");
        Ok(())
    }
    
    /// Get tenant for an agent
    pub fn get_agent_tenant(&self, agent_id: &str) -> Option<String> {
        self.agent_tenants.get(agent_id).map(|t| t.clone())
    }
    
    /// Record message for tenant
    pub fn record_message(&self, tenant_id: &str, tokens: u64) -> Result<(), TenantError> {
        let tenant = self.tenants.get(tenant_id)
            .ok_or_else(|| TenantError::NotFound(tenant_id.to_string()))?;
        
        if !tenant.is_active() {
            return Err(TenantError::Suspended(tenant_id.to_string()));
        }
        
        if let Some(mut usage) = self.usage.get_mut(tenant_id) {
            usage.check_quota(&tenant.quotas)?;
            usage.messages_today += 1;
            usage.tokens_today += tokens;
        }
        
        Ok(())
    }
    
    /// Get tenant usage
    pub fn get_usage(&self, tenant_id: &str) -> Option<TenantUsage> {
        self.usage.get(tenant_id).map(|u| u.clone())
    }
    
    /// Reset daily usage for all tenants
    pub fn reset_daily_usage(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64;
        
        for mut entry in self.usage.iter_mut() {
            entry.messages_today = 0;
            entry.tokens_today = 0;
            entry.last_reset = now;
        }
        info!("Reset daily usage for all tenants");
    }
    
    /// List all tenants
    pub fn list_tenants(&self) -> Vec<Tenant> {
        self.tenants.iter().map(|t| t.clone()).collect()
    }
    
    /// Delete a tenant
    pub fn delete_tenant(&self, tenant_id: &str) -> bool {
        self.tenants.remove(tenant_id);
        self.usage.remove(tenant_id);
        // Remove agent mappings
        self.agent_tenants.retain(|_, v| v != tenant_id);
        true
    }
}

impl Default for TenantManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_tenant() {
        let manager = TenantManager::new();
        let tenant = manager.create_tenant(
            "tenant-1".into(),
            "Test Tenant".into(),
            TenantTier::Starter,
        ).unwrap();
        
        assert_eq!(tenant.id, "tenant-1");
        assert_eq!(tenant.tier, TenantTier::Starter);
        assert!(tenant.is_active());
    }
    
    #[test]
    fn test_agent_registration() {
        let manager = TenantManager::new();
        manager.create_tenant("t1".into(), "Tenant 1".into(), TenantTier::Free).unwrap();
        
        manager.register_agent("agent-1", "t1").unwrap();
        let tenant = manager.get_agent_tenant("agent-1");
        assert_eq!(tenant, Some("t1".to_string()));
    }
    
    #[test]
    fn test_quota_check() {
        let manager = TenantManager::new();
        manager.create_tenant("t1".into(), "Test".into(), TenantTier::Free).unwrap();
        
        // Free tier has 1000 messages/day limit
        for _ in 0..1000 {
            manager.record_message("t1", 10).unwrap();
        }
        
        // Should fail now
        let result = manager.record_message("t1", 10);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_suspend_tenant() {
        let manager = TenantManager::new();
        manager.create_tenant("t1".into(), "Test".into(), TenantTier::Free).unwrap();
        
        manager.suspend_tenant("t1").unwrap();
        
        // Should fail to register agent
        let result = manager.register_agent("agent-1", "t1");
        assert!(matches!(result, Err(TenantError::Suspended(_))));
    }
}
