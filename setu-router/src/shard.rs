//! Shard routing abstraction for future multi-shard support
//!
//! Borrowed from colleague's design with enhancements.
//!
//! MVP: Single shard with a default shard ID.
//! Future: Multiple shards with resource-based routing.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use parking_lot::RwLock;
use tracing::debug;

/// Unique identifier for a shard
pub type ShardId = String;

/// Default shard ID for MVP (single shard)
pub const DEFAULT_SHARD_ID: &str = "default";

/// Shard configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardConfig {
    /// Shard identifier
    pub id: ShardId,
    
    /// Human-readable name
    pub name: String,
    
    /// Maximum number of solvers in this shard
    pub max_solvers: usize,
    
    /// Resource prefixes this shard handles (e.g., "account:", "coin:")
    /// Empty means shard can handle any resource
    pub resource_prefixes: Vec<String>,
}

impl ShardConfig {
    /// Create default shard config for MVP
    pub fn default_mvp() -> Self {
        Self {
            id: DEFAULT_SHARD_ID.to_string(),
            name: "Default Shard".to_string(),
            max_solvers: 6,
            resource_prefixes: vec![],
        }
    }
    
    /// Create a new shard config
    pub fn new(id: ShardId, name: String) -> Self {
        Self {
            id,
            name,
            max_solvers: 10,
            resource_prefixes: vec![],
        }
    }
    
    /// Set resource prefixes
    pub fn with_prefixes(mut self, prefixes: Vec<String>) -> Self {
        self.resource_prefixes = prefixes;
        self
    }
    
    /// Set max solvers
    pub fn with_max_solvers(mut self, max: usize) -> Self {
        self.max_solvers = max;
        self
    }
    
    /// Check if this shard can handle the given resource
    pub fn can_handle_resource(&self, resource: &str) -> bool {
        if self.resource_prefixes.is_empty() {
            return true;
        }
        self.resource_prefixes.iter().any(|prefix| resource.starts_with(prefix))
    }
}

impl Default for ShardConfig {
    fn default() -> Self {
        Self::default_mvp()
    }
}

// =============================================================================
// Shard Router Trait
// =============================================================================

/// Trait for shard routing strategy
/// 
/// Implement this trait to create custom shard routing logic.
/// In MVP, all transactions go to the default shard.
/// In the future, implement this trait to route based on resources.
pub trait ShardRouter: Send + Sync {
    /// Route resources to a shard ID
    fn route(&self, resources: &[String]) -> ShardId;
    
    /// Get all available shard IDs
    fn available_shards(&self) -> Vec<ShardId>;
    
    /// Check if a shard exists
    fn has_shard(&self, shard_id: &ShardId) -> bool;
}

// =============================================================================
// Single Shard Router (MVP)
// =============================================================================

/// Simple single-shard router for MVP
/// 
/// All transactions are routed to the default shard.
#[derive(Debug, Clone, Default)]
pub struct SingleShardRouter;

impl ShardRouter for SingleShardRouter {
    fn route(&self, _resources: &[String]) -> ShardId {
        DEFAULT_SHARD_ID.to_string()
    }
    
    fn available_shards(&self) -> Vec<ShardId> {
        vec![DEFAULT_SHARD_ID.to_string()]
    }
    
    fn has_shard(&self, shard_id: &ShardId) -> bool {
        shard_id == DEFAULT_SHARD_ID
    }
}

// =============================================================================
// Multi-Shard Router (Future)
// =============================================================================

/// Multi-shard router with resource-based routing
/// 
/// Routes transactions to shards based on resource prefixes.
pub struct MultiShardRouter {
    /// Shard configurations
    shards: RwLock<HashMap<ShardId, ShardConfig>>,
    
    /// Default shard for unmatched resources
    default_shard: ShardId,
}

impl MultiShardRouter {
    /// Create a new multi-shard router
    pub fn new() -> Self {
        let mut shards = HashMap::new();
        shards.insert(DEFAULT_SHARD_ID.to_string(), ShardConfig::default_mvp());
        
        Self {
            shards: RwLock::new(shards),
            default_shard: DEFAULT_SHARD_ID.to_string(),
        }
    }
    
    /// Add a shard
    pub fn add_shard(&self, config: ShardConfig) {
        debug!(
            shard_id = %config.id,
            name = %config.name,
            prefixes = ?config.resource_prefixes,
            "Adding shard"
        );
        let mut shards = self.shards.write();
        shards.insert(config.id.clone(), config);
    }
    
    /// Remove a shard
    pub fn remove_shard(&self, shard_id: &ShardId) {
        if shard_id != DEFAULT_SHARD_ID {
            debug!(shard_id = %shard_id, "Removing shard");
            let mut shards = self.shards.write();
            shards.remove(shard_id);
        }
    }
    
    /// Get shard config
    pub fn get_shard(&self, shard_id: &ShardId) -> Option<ShardConfig> {
        let shards = self.shards.read();
        shards.get(shard_id).cloned()
    }
    
    /// Set default shard
    pub fn set_default_shard(&mut self, shard_id: ShardId) {
        self.default_shard = shard_id;
    }
}

impl Default for MultiShardRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl ShardRouter for MultiShardRouter {
    fn route(&self, resources: &[String]) -> ShardId {
        if resources.is_empty() {
            return self.default_shard.clone();
        }
        
        let shards = self.shards.read();
        
        // Find the first shard that can handle the first resource
        for resource in resources {
            for (shard_id, config) in shards.iter() {
                if config.can_handle_resource(resource) && !config.resource_prefixes.is_empty() {
                    return shard_id.clone();
                }
            }
        }
        
        // Fallback to default shard
        self.default_shard.clone()
    }
    
    fn available_shards(&self) -> Vec<ShardId> {
        let shards = self.shards.read();
        shards.keys().cloned().collect()
    }
    
    fn has_shard(&self, shard_id: &ShardId) -> bool {
        let shards = self.shards.read();
        shards.contains_key(shard_id)
    }
}

impl std::fmt::Debug for MultiShardRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let shards = self.shards.read();
        f.debug_struct("MultiShardRouter")
            .field("shard_count", &shards.len())
            .field("default_shard", &self.default_shard)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_shard_config() {
        let config = ShardConfig::default_mvp();
        assert_eq!(config.id, DEFAULT_SHARD_ID);
        assert_eq!(config.max_solvers, 6);
        assert!(config.resource_prefixes.is_empty());
    }

    #[test]
    fn test_shard_can_handle_resource() {
        let config = ShardConfig::new("shard-1".to_string(), "Shard 1".to_string())
            .with_prefixes(vec!["account:".to_string(), "coin:".to_string()]);
        
        assert!(config.can_handle_resource("account:alice"));
        assert!(config.can_handle_resource("coin:btc"));
        assert!(!config.can_handle_resource("nft:token1"));
    }

    #[test]
    fn test_single_shard_router() {
        let router = SingleShardRouter;
        
        // All resources route to default shard
        assert_eq!(router.route(&["account:alice".to_string()]), DEFAULT_SHARD_ID);
        assert_eq!(router.route(&["nft:token1".to_string()]), DEFAULT_SHARD_ID);
        assert_eq!(router.route(&[]), DEFAULT_SHARD_ID);
        
        assert!(router.has_shard(&DEFAULT_SHARD_ID.to_string()));
        assert!(!router.has_shard(&"other".to_string()));
    }

    #[test]
    fn test_multi_shard_router() {
        let router = MultiShardRouter::new();
        
        // Add shards with specific prefixes
        router.add_shard(
            ShardConfig::new("shard-accounts".to_string(), "Accounts Shard".to_string())
                .with_prefixes(vec!["account:".to_string()])
        );
        
        router.add_shard(
            ShardConfig::new("shard-nft".to_string(), "NFT Shard".to_string())
                .with_prefixes(vec!["nft:".to_string()])
        );
        
        // Route based on resource prefix
        assert_eq!(
            router.route(&["account:alice".to_string()]),
            "shard-accounts"
        );
        assert_eq!(
            router.route(&["nft:token1".to_string()]),
            "shard-nft"
        );
        
        // Unknown prefix routes to default
        assert_eq!(
            router.route(&["unknown:resource".to_string()]),
            DEFAULT_SHARD_ID
        );
    }

    #[test]
    fn test_multi_shard_router_available_shards() {
        let router = MultiShardRouter::new();
        router.add_shard(ShardConfig::new("shard-1".to_string(), "Shard 1".to_string()));
        router.add_shard(ShardConfig::new("shard-2".to_string(), "Shard 2".to_string()));
        
        let shards = router.available_shards();
        assert_eq!(shards.len(), 3); // default + 2 added
        assert!(shards.contains(&DEFAULT_SHARD_ID.to_string()));
        assert!(shards.contains(&"shard-1".to_string()));
        assert!(shards.contains(&"shard-2".to_string()));
    }
}

