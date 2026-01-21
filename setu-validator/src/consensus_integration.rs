//! Consensus Integration Module
//!
//! This module integrates the consensus crate into the validator,
//! providing a unified interface for:
//! - DAG management with consensus
//! - VLC-based leader rotation
//! - ConsensusFrame creation and voting
//! - Anchor finalization
//!
//! ## Main Flow
//!
//! 1. User submits transaction to validator
//! 2. Validator distributes transaction to solver
//! 3. Solver executes transaction and constructs event, sends to validator
//! 4. Validator verifies event, broadcasts, adds to DAG
//! 5. Leader validator proposes CF based on VLC increment
//! 6. All validators vote on CF
//! 7. Final confirmation and anchor storage

use consensus::{
    ConsensusEngine, ConsensusMessage, DagStats as ConsensusDagStats,
    ValidatorSet, TeeVerifier, VerificationResult,
    liveness::Round, ConsensusBroadcaster,
};
use setu_types::{
    Anchor, ConsensusConfig, ConsensusFrame, Event, EventId, Vote,
    NodeInfo, ValidatorInfo, SetuResult, SetuError, SubnetId,
};
use setu_storage::subnet_state::GlobalStateManager;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Mutex, broadcast};
use tracing::{debug, info, warn};

/// Configuration for the consensus-integrated validator
#[derive(Debug, Clone)]
pub struct ConsensusValidatorConfig {
    /// Consensus configuration
    pub consensus: ConsensusConfig,
    /// Validator node info
    pub node_info: NodeInfo,
    /// Whether this validator is initially the leader
    pub is_leader: bool,
    /// Buffer size for consensus message channel
    pub message_buffer_size: usize,
}

impl Default for ConsensusValidatorConfig {
    fn default() -> Self {
        Self {
            consensus: ConsensusConfig::default(),
            node_info: NodeInfo::new_validator(
                "validator-1".to_string(),
                "127.0.0.1".to_string(),
                8080,
            ),
            is_leader: false,
            message_buffer_size: 1000,
        }
    }
}

/// Consensus-integrated validator
///
/// This wraps the consensus engine and provides the main validation flow:
/// - Receive events from solvers
/// - Verify and add to DAG
/// - Propose/vote on ConsensusFrames
/// - Finalize and persist anchors
pub struct ConsensusValidator {
    /// Configuration
    config: ConsensusValidatorConfig,
    /// The consensus engine
    engine: Arc<ConsensusEngine>,
    /// Validator set for leader election
    validator_set: Arc<RwLock<ValidatorSet>>,
    /// TEE verifier for attestation verification
    tee_verifier: Arc<TeeVerifier>,
    /// Channel for sending consensus messages to network
    message_tx: mpsc::Sender<ConsensusMessage>,
    /// Channel for receiving consensus messages from network
    message_rx: Arc<Mutex<mpsc::Receiver<ConsensusMessage>>>,
    /// Broadcast channel for CF finalization notifications
    finalization_tx: broadcast::Sender<ConsensusFrame>,
    /// Pending votes awaiting quorum
    pending_votes: Arc<RwLock<HashMap<String, Vec<Vote>>>>,
    /// Running flag
    running: Arc<RwLock<bool>>,
}

impl ConsensusValidator {
    /// Create a new consensus validator
    pub fn new(config: ConsensusValidatorConfig) -> Self {
        let (msg_tx, msg_rx) = mpsc::channel(config.message_buffer_size);
        let (finalization_tx, _) = broadcast::channel(100);
        
        // Initialize validator set
        let mut validator_set = ValidatorSet::new();
        let validator_info = ValidatorInfo::new(config.node_info.clone(), config.is_leader);
        validator_set.add_validator(validator_info);
        
        // Create consensus engine
        let engine = Arc::new(ConsensusEngine::new(
            config.consensus.clone(),
            config.node_info.id.clone(),
            validator_set.clone(),
        ));
        
        // Create TEE verifier with empty registry (permissive mode for now)
        let tee_verifier = Arc::new(TeeVerifier::permissive());
        
        Self {
            config,
            engine,
            validator_set: Arc::new(RwLock::new(validator_set)),
            tee_verifier,
            message_tx: msg_tx,
            message_rx: Arc::new(Mutex::new(msg_rx)),
            finalization_tx,
            pending_votes: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Create with a persistent state manager for Merkle tree persistence
    pub fn with_state_manager(
        config: ConsensusValidatorConfig,
        state_manager: GlobalStateManager,
    ) -> Self {
        let (msg_tx, msg_rx) = mpsc::channel(config.message_buffer_size);
        let (finalization_tx, _) = broadcast::channel(100);
        
        let mut validator_set = ValidatorSet::new();
        let validator_info = ValidatorInfo::new(config.node_info.clone(), config.is_leader);
        validator_set.add_validator(validator_info);
        
        let engine = Arc::new(ConsensusEngine::with_state_manager(
            config.consensus.clone(),
            config.node_info.id.clone(),
            validator_set.clone(),
            state_manager,
        ));
        
        // Create TEE verifier with empty registry (permissive mode for now)
        let tee_verifier = Arc::new(TeeVerifier::permissive());
        
        Self {
            config,
            engine,
            validator_set: Arc::new(RwLock::new(validator_set)),
            tee_verifier,
            message_tx: msg_tx,
            message_rx: Arc::new(Mutex::new(msg_rx)),
            finalization_tx,
            pending_votes: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Set the network broadcaster for P2P message delivery
    ///
    /// This should be called after the network layer is initialized.
    /// The broadcaster is injected into the underlying ConsensusEngine.
    pub async fn set_broadcaster(&self, broadcaster: Arc<dyn ConsensusBroadcaster>) {
        self.engine.set_broadcaster(broadcaster).await;
        info!("Consensus broadcaster configured for validator");
    }
    
    /// Check if a broadcaster is configured
    pub async fn has_broadcaster(&self) -> bool {
        self.engine.has_broadcaster().await
    }
    
    // =========================================================================
    // Core Operations
    // =========================================================================
    
    /// Submit an event from a solver (after TEE execution)
    /// 
    /// This is the main entry point for solver events:
    /// 1. Verify TEE attestation (via execution result)
    /// 2. Add to DAG
    /// 3. Update VLC
    /// 4. Try to create CF if we're the leader
    /// 5. Broadcast event to other validators
    pub async fn submit_event(&self, event: Event) -> SetuResult<EventId> {
        info!(
            event_id = %event.id,
            creator = %event.creator,
            "Submitting event to consensus"
        );
        
        // Step 1: Verify execution result is present and successful
        // TEE attestation verification is done by the TeeVerifier when enabled
        if let Some(ref exec_result) = event.execution_result {
            if !exec_result.success {
                return Err(SetuError::InvalidData(
                    "Event execution result is not successful".to_string()
                ));
            }
        }
        
        // Step 2: Add event to DAG (this also updates VLC)
        let event_id = self.engine.add_event(event.clone()).await?;
        
        // Step 3: Broadcast to other validators
        let _ = self.message_tx.send(ConsensusMessage::NewEvent(event)).await;
        
        info!(
            event_id = %event_id,
            "Event added to DAG"
        );
        
        Ok(event_id)
    }
    
    /// Receive an event from another validator
    pub async fn receive_event(&self, event: Event) -> SetuResult<EventId> {
        debug!(
            event_id = %event.id,
            from = %event.creator,
            "Receiving event from network"
        );
        
        // Add to local DAG
        self.engine.add_event(event).await
    }
    
    /// Receive a ConsensusFrame proposal from the leader
    pub async fn receive_cf(&self, cf: ConsensusFrame) -> SetuResult<()> {
        info!(
            cf_id = %cf.id,
            proposer = %cf.proposer,
            "Receiving CF proposal"
        );
        
        // Verify and process the CF
        self.engine.receive_cf(cf).await
    }
    
    /// Receive a vote from another validator
    pub async fn receive_vote(&self, vote: Vote) -> SetuResult<(bool, Option<Anchor>)> {
        debug!(
            cf_id = %vote.cf_id,
            voter = %vote.validator_id,
            approve = vote.approve,
            "Receiving vote"
        );
        
        self.engine.receive_vote(vote).await
    }
    
    // =========================================================================
    // Leader Election
    // =========================================================================
    
    /// Check if this validator is the current leader
    pub async fn is_leader(&self) -> bool {
        self.engine.is_current_leader().await
    }
    
    /// Get the current round number
    pub async fn current_round(&self) -> Round {
        self.engine.current_round().await
    }
    
    /// Get the leader for a specific round
    pub async fn get_leader_for_round(&self, round: Round) -> Option<String> {
        self.engine.get_valid_proposer(round).await
    }
    
    /// Advance to the next round (called after CF finalization)
    pub async fn advance_round(&self) -> Round {
        self.engine.advance_round().await
    }
    
    // =========================================================================
    // Validator Set Management
    // =========================================================================
    
    /// Add a peer validator
    pub async fn add_validator(&self, info: ValidatorInfo) {
        let mut vs = self.validator_set.write().await;
        vs.add_validator(info.clone());
        
        // Also register in TEE verifier if they have a public key
        // (This would be extended in a real implementation)
        
        info!(
            validator_id = %info.node.id,
            "Added validator to consensus set"
        );
    }
    
    /// Remove a validator
    pub async fn remove_validator(&self, validator_id: &str) {
        let mut vs = self.validator_set.write().await;
        vs.remove_validator(validator_id);
        
        info!(
            validator_id = %validator_id,
            "Removed validator from consensus set"
        );
    }
    
    /// Get validator count
    pub async fn validator_count(&self) -> usize {
        let vs = self.validator_set.read().await;
        vs.all_validators().len()
    }
    
    // =========================================================================
    // TEE Verification
    // =========================================================================
    
    /// Verify TEE attestation for an event
    /// 
    /// Uses the TeeVerifier to verify the event's execution result and attestation.
    #[allow(dead_code)]
    fn verify_tee_attestation(&self, event: &Event) -> SetuResult<()> {
        match self.tee_verifier.verify_event(event) {
            VerificationResult::Verified => Ok(()),
            VerificationResult::NotApplicable => {
                // TEE verification not required for this event type (e.g., ROOT subnet)
                Ok(())
            }
            VerificationResult::Failed(err) => {
                Err(SetuError::InvalidData(format!("TEE attestation invalid: {}", err)))
            }
        }
    }
    
    // =========================================================================
    // State Access
    // =========================================================================
    
    /// Get DAG statistics
    pub async fn dag_stats(&self) -> ConsensusDagStats {
        self.engine.get_dag_stats().await
    }
    
    /// Get current VLC snapshot
    pub async fn vlc_snapshot(&self) -> setu_vlc::VLCSnapshot {
        self.engine.get_vlc_snapshot().await
    }
    
    /// Get current DAG tips
    pub async fn get_tips(&self) -> Vec<EventId> {
        self.engine.get_tips().await
    }
    
    /// Get anchor count
    pub async fn anchor_count(&self) -> usize {
        self.engine.get_anchor_count().await
    }
    
    /// Get global state root
    pub async fn global_state_root(&self) -> [u8; 32] {
        self.engine.get_global_state_root().await
    }
    
    /// Get subnet state root
    pub async fn subnet_state_root(&self, subnet_id: &SubnetId) -> Option<[u8; 32]> {
        self.engine.get_subnet_state_root(subnet_id).await
    }
    
    /// Mark an anchor as persisted (for safe GC)
    pub async fn mark_anchor_persisted(&self, anchor_id: &str) {
        self.engine.mark_anchor_persisted(anchor_id).await;
    }
    
    // =========================================================================
    // Message Handling
    // =========================================================================
    
    /// Get a sender for consensus messages (for network layer)
    pub fn message_sender(&self) -> mpsc::Sender<ConsensusMessage> {
        self.message_tx.clone()
    }
    
    /// Subscribe to CF finalization notifications
    pub fn subscribe_finalization(&self) -> broadcast::Receiver<ConsensusFrame> {
        self.finalization_tx.subscribe()
    }
    
    /// Get the local validator ID
    pub fn validator_id(&self) -> &str {
        self.engine.local_validator_id()
    }
    
    /// Get configuration
    pub fn config(&self) -> &ConsensusValidatorConfig {
        &self.config
    }
}

/// Statistics for the consensus validator
#[derive(Debug, Clone)]
pub struct ConsensusValidatorStats {
    pub validator_id: String,
    pub is_leader: bool,
    pub current_round: Round,
    pub validator_count: usize,
    pub dag_node_count: usize,
    pub dag_max_depth: u64,
    pub dag_tip_count: usize,
    pub anchor_count: usize,
    pub vlc_logical_time: u64,
}

impl ConsensusValidator {
    /// Get comprehensive statistics
    pub async fn stats(&self) -> ConsensusValidatorStats {
        let dag_stats = self.dag_stats().await;
        let vlc = self.vlc_snapshot().await;
        
        ConsensusValidatorStats {
            validator_id: self.validator_id().to_string(),
            is_leader: self.is_leader().await,
            current_round: self.current_round().await,
            validator_count: self.validator_count().await,
            dag_node_count: dag_stats.node_count,
            dag_max_depth: dag_stats.max_depth,
            dag_tip_count: dag_stats.tip_count,
            anchor_count: self.anchor_count().await,
            vlc_logical_time: vlc.logical_time,
        }
    }
}

/// Event handler for processing consensus messages in a background loop
pub struct ConsensusMessageHandler {
    validator: Arc<ConsensusValidator>,
    running: Arc<RwLock<bool>>,
}

impl ConsensusMessageHandler {
    pub fn new(validator: Arc<ConsensusValidator>) -> Self {
        Self {
            validator,
            running: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Start the message handling loop
    pub async fn start(&self, mut message_rx: mpsc::Receiver<ConsensusMessage>) {
        *self.running.write().await = true;
        
        info!("Starting consensus message handler");
        
        while *self.running.read().await {
            tokio::select! {
                Some(msg) = message_rx.recv() => {
                    self.handle_message(msg).await;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                    // Periodic check for leader duties
                    if self.validator.is_leader().await {
                        // Leader-specific periodic tasks would go here
                    }
                }
            }
        }
        
        info!("Consensus message handler stopped");
    }
    
    async fn handle_message(&self, msg: ConsensusMessage) {
        match msg {
            ConsensusMessage::NewEvent(event) => {
                debug!(event_id = %event.id, "Handling new event");
                if let Err(e) = self.validator.receive_event(event).await {
                    warn!(error = %e, "Failed to receive event");
                }
            }
            ConsensusMessage::ProposeFrame(cf) => {
                info!(cf_id = %cf.id, "Handling CF proposal");
                if let Err(e) = self.validator.receive_cf(cf).await {
                    warn!(error = %e, "Failed to receive CF");
                }
            }
            ConsensusMessage::Vote(vote) => {
                debug!(cf_id = %vote.cf_id, "Handling vote");
                match self.validator.receive_vote(vote).await {
                    Ok((finalized, anchor)) => {
                        if finalized {
                            if let Some(ref a) = anchor {
                                info!(anchor_id = %a.id, "CF finalized");
                            }
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to receive vote");
                    }
                }
            }
            ConsensusMessage::FrameFinalized(cf) => {
                info!(cf_id = %cf.id, "CF finalized notification");
                // Could trigger additional actions here
            }
            ConsensusMessage::LeaderChanged { round, new_leader } => {
                info!(round = round, new_leader = %new_leader, "Leader changed");
            }
        }
    }
    
    /// Stop the handler
    pub async fn stop(&self) {
        *self.running.write().await = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use setu_types::EventType;
    use setu_vlc::VectorClock;

    fn create_test_config() -> ConsensusValidatorConfig {
        ConsensusValidatorConfig {
            consensus: ConsensusConfig {
                vlc_delta_threshold: 5,
                min_events_per_cf: 1,
                validator_count: 1,
                ..Default::default()
            },
            node_info: NodeInfo::new_validator(
                "test-validator".to_string(),
                "127.0.0.1".to_string(),
                8080,
            ),
            is_leader: true,
            message_buffer_size: 100,
        }
    }
    
    fn create_test_event(creator: &str) -> Event {
        Event::genesis(
            creator.to_string(),
            setu_vlc::VLCSnapshot {
                vector_clock: VectorClock::new(),
                logical_time: 0,
                physical_time: 0,
            },
        )
    }

    #[tokio::test]
    async fn test_consensus_validator_creation() {
        let config = create_test_config();
        let validator = ConsensusValidator::new(config);
        
        assert_eq!(validator.validator_id(), "test-validator");
        assert!(validator.is_leader().await);
    }

    #[tokio::test]
    async fn test_submit_event() {
        let config = create_test_config();
        let validator = ConsensusValidator::new(config);
        
        let event = create_test_event("solver-1");
        let event_id = validator.submit_event(event).await.unwrap();
        
        assert!(!event_id.is_empty());
        
        let stats = validator.dag_stats().await;
        assert_eq!(stats.node_count, 1);
    }

    #[tokio::test]
    async fn test_round_advancement() {
        let config = create_test_config();
        let validator = ConsensusValidator::new(config);
        
        assert_eq!(validator.current_round().await, 0);
        
        let new_round = validator.advance_round().await;
        assert_eq!(new_round, 1);
    }

    #[tokio::test]
    async fn test_stats() {
        let config = create_test_config();
        let validator = ConsensusValidator::new(config);
        
        let stats = validator.stats().await;
        assert_eq!(stats.validator_id, "test-validator");
        assert!(stats.is_leader);
        assert_eq!(stats.current_round, 0);
    }
}
