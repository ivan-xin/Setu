//! Setu Solver - Execution node
//!
//! The solver is responsible for:
//! - Receiving Transfer intents
//! - Executing computations
//! - Generating events
//! - Broadcasting to the network

use setu_core::{NodeConfig, ShardManager};
use std::sync::Arc;
use tracing::info;

/// Solver node
pub struct Solver {
    config: NodeConfig,
    shard_manager: Arc<ShardManager>,
}

impl Solver {
    /// Create a new solver
    pub fn new(config: NodeConfig) -> Self {
        info!(
            node_id = %config.node_id,
            "Creating solver node"
        );
        
        let shard_manager = Arc::new(ShardManager::new());
        
        Self {
            config,
            shard_manager,
        }
    }
    
    /// Run the solver
    pub async fn run(self) {
        info!(
            node_id = %self.config.node_id,
            port = self.config.network.port,
            "Solver started"
        );
        
        // TODO: Implement solver logic
        // - Listen for Transfer intents
        // - Execute transfers
        // - Generate events
        // - Broadcast events
        
        // For now, just keep running
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }
    
    /// Get node ID
    pub fn node_id(&self) -> &str {
        &self.config.node_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_solver_creation() {
        let config = NodeConfig::default();
        let solver = Solver::new(config);
        assert!(!solver.node_id().is_empty());
    }
}

