//! Setu Solver - Execution node
//!
//! The solver is responsible for:
//! - Receiving Transfer intents
//! - Executing computations
//! - Generating events
//! - Broadcasting to the network

use core_types::Transfer;
use setu_core::{NodeConfig, ShardManager};
use setu_types::event::{Event, EventType, ExecutionResult, StateChange};
use setu_vlc::VLCSnapshot;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, error};

/// Solver node
pub struct Solver {
    config: NodeConfig,
    shard_manager: Arc<ShardManager>,
    transfer_rx: mpsc::UnboundedReceiver<Transfer>,
    event_tx: mpsc::UnboundedSender<Event>,
}

impl Solver {
    /// Create a new solver with channels
    pub fn new(
        config: NodeConfig,
        transfer_rx: mpsc::UnboundedReceiver<Transfer>,
        event_tx: mpsc::UnboundedSender<Event>,
    ) -> Self {
        info!(
            node_id = %config.node_id,
            "Creating solver node"
        );
        
        let shard_manager = Arc::new(ShardManager::new());
        
        Self {
            config,
            shard_manager,
            transfer_rx,
            event_tx,
        }
    }
    
    /// Run the solver
    pub async fn run(mut self) {
        info!(
            node_id = %self.config.node_id,
            port = self.config.network.port,
            "Solver started, waiting for transfers..."
        );
        
        // Main loop: receive transfers and process them
        while let Some(transfer) = self.transfer_rx.recv().await {
            info!(
                transfer_id = %transfer.id,
                from = %transfer.from,
                to = %transfer.to,
                amount = %transfer.amount,
                "Received transfer"
            );
            
            // Execute the transfer
            match self.execute_transfer(&transfer).await {
                Ok(event) => {
                    info!(
                        event_id = %event.id,
                        "Transfer executed successfully, generated event"
                    );
                    
                    // Send event to validator
                    if let Err(e) = self.event_tx.send(event) {
                        error!("Failed to send event: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to execute transfer: {}", e);
                }
            }
        }
        
        info!("Solver stopped");
    }
    
    /// Execute a transfer and generate an event
    async fn execute_transfer(&self, transfer: &Transfer) -> anyhow::Result<Event> {
        info!("Executing transfer: {}", transfer.id);
        
        // Simulate some work
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        // Create VLC snapshot
        let vlc_snapshot = VLCSnapshot {
            vector_clock: setu_vlc::VectorClock::new(),
            logical_time: transfer.vlc.entries.values().sum(),
            physical_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as u64,
        };
        
        // Create execution result
        let execution_result = ExecutionResult {
            success: true,
            message: Some(format!("Transfer {} executed successfully", transfer.id)),
            state_changes: vec![
                StateChange {
                    key: format!("balance:{}", transfer.from),
                    old_value: Some(vec![]),
                    new_value: Some(vec![]),
                },
                StateChange {
                    key: format!("balance:{}", transfer.to),
                    old_value: Some(vec![]),
                    new_value: Some(vec![]),
                },
            ],
        };
        
        // Create event
        let mut event = Event::new(
            EventType::Transfer,
            vec![],
            vlc_snapshot,
            self.config.node_id.clone(),
        );
        
        // Attach transfer and execution result
        event = event.with_transfer(setu_types::event::Transfer {
            from: transfer.from.clone(),
            to: transfer.to.clone(),
            amount: transfer.amount as u64,
        });
        event.set_execution_result(execution_result);
        
        Ok(event)
    }
    
    /// Get node ID
    pub fn node_id(&self) -> &str {
        &self.config.node_id
    }
}
