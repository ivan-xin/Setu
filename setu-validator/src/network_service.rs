//! Network service for Validator
//!
//! This module provides the Anemo-based network service for the Validator,
//! handling RPC requests for registration and other operations.

use crate::RouterManager;
use axum::{
    routing::{get, post},
    Router, Json,
    extract::State,
};
use core_types::{Transfer, TransferType, Vlc};
use parking_lot::RwLock;
use setu_rpc::{
    RegisterSolverRequest, RegisterSolverResponse,
    RegisterValidatorRequest, RegisterValidatorResponse,
    UnregisterRequest, UnregisterResponse,
    HeartbeatRequest, HeartbeatResponse,
    GetSolverListRequest, GetSolverListResponse,
    GetValidatorListRequest, GetValidatorListResponse,
    GetNodeStatusRequest, GetNodeStatusResponse,
    SubmitTransferRequest, SubmitTransferResponse,
    GetTransferStatusRequest, GetTransferStatusResponse,
    ProcessingStep,
    SolverListItem, ValidatorListItem, NodeType,
    RegistrationHandler,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{info, warn, debug, error};

/// Validator info for registration
#[derive(Debug, Clone)]
pub struct ValidatorInfo {
    pub validator_id: String,
    pub address: String,
    pub port: u16,
    pub status: String,
    pub registered_at: u64,
}

/// Network service configuration
#[derive(Debug, Clone)]
pub struct NetworkServiceConfig {
    /// Listen address for HTTP API
    pub http_listen_addr: SocketAddr,
    /// Listen address for Anemo P2P
    pub p2p_listen_addr: SocketAddr,
}

impl Default for NetworkServiceConfig {
    fn default() -> Self {
        Self {
            http_listen_addr: "127.0.0.1:8080".parse().unwrap(),
            p2p_listen_addr: "127.0.0.1:9000".parse().unwrap(),
        }
    }
}

/// Validator network service
/// 
/// Handles incoming RPC requests for:
/// - Solver registration
/// - Validator registration  
/// - Heartbeat
/// - Status queries
/// - Transfer submission
pub struct ValidatorNetworkService {
    /// Validator ID
    validator_id: String,
    
    /// Router manager for solver management
    router_manager: Arc<RouterManager>,
    
    /// Registered validators (other validators in the network)
    validators: Arc<RwLock<HashMap<String, ValidatorInfo>>>,
    
    /// Solver channels for sending transfers
    solver_channels: Arc<RwLock<HashMap<String, mpsc::UnboundedSender<Transfer>>>>,
    
    /// Transfer tracking (transfer_id -> status)
    transfer_status: Arc<RwLock<HashMap<String, TransferTracker>>>,
    
    /// Configuration
    config: NetworkServiceConfig,
    
    /// Start time for uptime calculation
    start_time: u64,
    
    /// Transfer counter for generating IDs
    transfer_counter: AtomicU64,
    
    /// VLC counter (simulated)
    vlc_counter: AtomicU64,
}

/// Transfer tracking information
#[derive(Debug, Clone)]
pub struct TransferTracker {
    pub transfer_id: String,
    pub status: String,
    pub solver_id: Option<String>,
    pub event_id: Option<String>,
    pub processing_steps: Vec<ProcessingStep>,
    pub created_at: u64,
}

impl ValidatorNetworkService {
    /// Create a new validator network service
    pub fn new(
        validator_id: String,
        router_manager: Arc<RouterManager>,
        config: NetworkServiceConfig,
    ) -> Self {
        let start_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        info!(
            validator_id = %validator_id,
            http_addr = %config.http_listen_addr,
            p2p_addr = %config.p2p_listen_addr,
            "Creating validator network service"
        );
        
        Self {
            validator_id,
            router_manager,
            validators: Arc::new(RwLock::new(HashMap::new())),
            solver_channels: Arc::new(RwLock::new(HashMap::new())),
            transfer_status: Arc::new(RwLock::new(HashMap::new())),
            config,
            start_time,
            transfer_counter: AtomicU64::new(0),
            vlc_counter: AtomicU64::new(0),
        }
    }
    
    /// Get the registration handler for RPC server
    pub fn registration_handler(self: &Arc<Self>) -> Arc<ValidatorRegistrationHandler> {
        Arc::new(ValidatorRegistrationHandler {
            service: self.clone(),
        })
    }
    
    /// Start the HTTP API server
    pub async fn start_http_server(self: Arc<Self>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let service = self.clone();
        
        let app = Router::new()
            .route("/api/v1/register/solver", post(http_register_solver))
            .route("/api/v1/register/validator", post(http_register_validator))
            .route("/api/v1/solvers", get(http_get_solvers))
            .route("/api/v1/validators", get(http_get_validators))
            .route("/api/v1/health", get(http_health))
            .route("/api/v1/transfer", post(http_submit_transfer))
            .route("/api/v1/transfer/status", post(http_get_transfer_status))
            .route("/api/v1/heartbeat", post(http_heartbeat))
            .with_state(service);
        
        let listener = tokio::net::TcpListener::bind(self.config.http_listen_addr).await?;
        
        info!(
            addr = %self.config.http_listen_addr,
            "HTTP API server started"
        );
        
        axum::serve(listener, app).await?;
        
        Ok(())
    }
    
    /// Submit a transfer (with simulated processing pipeline)
    pub async fn submit_transfer(&self, request: SubmitTransferRequest) -> SubmitTransferResponse {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        // Generate transfer ID
        let transfer_id = format!(
            "tx-{}-{}",
            now,
            self.transfer_counter.fetch_add(1, Ordering::SeqCst)
        );
        
        let mut steps = Vec::new();
        
        info!("╔════════════════════════════════════════════════════════════╗");
        info!("║              Processing Transfer: {}              ║", &transfer_id[..20.min(transfer_id.len())]);
        info!("╚════════════════════════════════════════════════════════════╝");
        
        // Step 1: Receive and validate
        info!("[STEP 1/7] 📥 Receiving transfer request...");
        info!("           From: {} → To: {}", request.from, request.to);
        info!("           Amount: {}", request.amount);
        info!("           Type: {}", request.transfer_type);
        steps.push(ProcessingStep {
            step: "receive".to_string(),
            status: "completed".to_string(),
            details: Some(format!("Transfer {} received", transfer_id)),
            timestamp: now,
        });
        
        // Step 2: VLC Assignment (Simulated)
        let vlc_time = self.vlc_counter.fetch_add(1, Ordering::SeqCst);
        info!("[STEP 2/7] ⏰ [VLC] Assigning Vector Logical Clock...");
        info!("           └─ VLC Time: {} (simulated)", vlc_time);
        info!("           └─ Node: {}", self.validator_id);
        steps.push(ProcessingStep {
            step: "vlc_assign".to_string(),
            status: "completed".to_string(),
            details: Some(format!("VLC time: {} (simulated)", vlc_time)),
            timestamp: now,
        });
        
        // Step 3: DAG Parent Resolution (Simulated)
        info!("[STEP 3/7] 🔗 [DAG] Resolving parent events...");
        info!("           └─ Checking resource dependencies: {:?}", request.resources);
        info!("           └─ Parent events: [] (genesis or no conflicts)");
        info!("           └─ DAG depth: 0 (simulated)");
        steps.push(ProcessingStep {
            step: "dag_resolve".to_string(),
            status: "completed".to_string(),
            details: Some("No parent conflicts (simulated)".to_string()),
            timestamp: now,
        });
        
        // Step 4: Router Selection
        info!("[STEP 4/7] 🔀 Selecting solver via router...");
        
        // Create internal transfer
        let mut vlc = Vlc::new();
        vlc.entries.insert(self.validator_id.clone(), vlc_time);
        
        let transfer_type = match request.transfer_type.to_lowercase().as_str() {
            "flux" | "fluxtransfer" => TransferType::FluxTransfer,
            _ => TransferType::FluxTransfer,
        };
        
        let transfer = Transfer {
            id: transfer_id.clone(),
            from: request.from.clone(),
            to: request.to.clone(),
            amount: request.amount,
            transfer_type,
            resources: if request.resources.is_empty() {
                vec![format!("account:{}", request.from), format!("account:{}", request.to)]
            } else {
                request.resources.clone()
            },
            vlc,
            power: 10,
            preferred_solver: request.preferred_solver.clone(),
            shard_id: request.shard_id.clone(),
        };
        
        // Route to solver
        let solver_result = self.router_manager.route_transfer(&transfer);
        
        let solver_id = match solver_result {
            Ok(id) => {
                info!("           └─ Selected solver: {}", id);
                steps.push(ProcessingStep {
                    step: "route".to_string(),
                    status: "completed".to_string(),
                    details: Some(format!("Routed to solver: {}", id)),
                    timestamp: now,
                });
                Some(id)
            }
            Err(e) => {
                warn!("           └─ No solver available: {}", e);
                steps.push(ProcessingStep {
                    step: "route".to_string(),
                    status: "failed".to_string(),
                    details: Some(format!("No solver available: {}", e)),
                    timestamp: now,
                });
                
                // Store failed transfer status
                self.transfer_status.write().insert(transfer_id.clone(), TransferTracker {
                    transfer_id: transfer_id.clone(),
                    status: "failed".to_string(),
                    solver_id: None,
                    event_id: None,
                    processing_steps: steps.clone(),
                    created_at: now,
                });
                
                return SubmitTransferResponse {
                    success: false,
                    message: format!("No solver available: {}", e),
                    transfer_id: Some(transfer_id),
                    solver_id: None,
                    processing_steps: steps,
                };
            }
        };
        
        // Step 5: Send to Solver
        info!("[STEP 5/7] 📤 Dispatching to solver...");
        if let Some(ref sid) = solver_id {
            match self.router_manager.send_to_solver(sid, transfer).await {
                Ok(()) => {
                    info!("           └─ Transfer dispatched successfully");
                    steps.push(ProcessingStep {
                        step: "dispatch".to_string(),
                        status: "completed".to_string(),
                        details: Some("Transfer sent to solver".to_string()),
                        timestamp: now,
                    });
                }
                Err(e) => {
                    error!("           └─ Dispatch failed: {}", e);
                    steps.push(ProcessingStep {
                        step: "dispatch".to_string(),
                        status: "failed".to_string(),
                        details: Some(format!("Dispatch error: {}", e)),
                        timestamp: now,
                    });
                }
            }
        }
        
        // Step 6: Consensus Preparation (Simulated)
        info!("[STEP 6/7] 🤝 [CONSENSUS] Preparing consensus...");
        info!("           └─ Mode: Single validator (no consensus needed)");
        info!("           └─ Validators: 1 (self)");
        info!("           └─ Threshold: 1/1 (100%)");
        steps.push(ProcessingStep {
            step: "consensus_prepare".to_string(),
            status: "completed".to_string(),
            details: Some("Single validator mode - no consensus needed (simulated)".to_string()),
            timestamp: now,
        });
        
        // Step 7: FoldGraph Update (Simulated)
        info!("[STEP 7/7] 📊 [FOLDGRAPH] Updating fold graph...");
        info!("           └─ Current frame: 0");
        info!("           └─ Events in frame: 1");
        info!("           └─ Fold threshold: 100 events");
        info!("           └─ Status: Not folding (below threshold)");
        steps.push(ProcessingStep {
            step: "foldgraph_update".to_string(),
            status: "completed".to_string(),
            details: Some("FoldGraph updated (simulated)".to_string()),
            timestamp: now,
        });
        
        info!("╔════════════════════════════════════════════════════════════╗");
        info!("║              Transfer Submitted Successfully               ║");
        info!("╠════════════════════════════════════════════════════════════╣");
        info!("║  Transfer ID: {:^44} ║", &transfer_id);
        info!("║  Solver:      {:^44} ║", solver_id.as_deref().unwrap_or("N/A"));
        info!("║  Status:      {:^44} ║", "pending_execution");
        info!("╚════════════════════════════════════════════════════════════╝");
        
        // Store transfer status
        self.transfer_status.write().insert(transfer_id.clone(), TransferTracker {
            transfer_id: transfer_id.clone(),
            status: "pending_execution".to_string(),
            solver_id: solver_id.clone(),
            event_id: None,
            processing_steps: steps.clone(),
            created_at: now,
        });
        
        SubmitTransferResponse {
            success: true,
            message: "Transfer submitted successfully".to_string(),
            transfer_id: Some(transfer_id),
            solver_id,
            processing_steps: steps,
        }
    }
    
    /// Get transfer status
    pub fn get_transfer_status(&self, transfer_id: &str) -> GetTransferStatusResponse {
        if let Some(tracker) = self.transfer_status.read().get(transfer_id) {
            GetTransferStatusResponse {
                found: true,
                transfer_id: tracker.transfer_id.clone(),
                status: Some(tracker.status.clone()),
                solver_id: tracker.solver_id.clone(),
                event_id: tracker.event_id.clone(),
                processing_steps: tracker.processing_steps.clone(),
            }
        } else {
            GetTransferStatusResponse {
                found: false,
                transfer_id: transfer_id.to_string(),
                status: None,
                solver_id: None,
                event_id: None,
                processing_steps: vec![],
            }
        }
    }
    
    /// Register a solver internally (creates channel)
    fn register_solver_internal(&self, request: &RegisterSolverRequest) -> mpsc::UnboundedSender<Transfer> {
        let (tx, rx) = mpsc::unbounded_channel();
        
        // Store the channel
        self.solver_channels.write().insert(request.solver_id.clone(), tx.clone());
        
        // Register with router manager
        self.router_manager.register_solver_with_affinity(
            request.solver_id.clone(),
            format!("{}:{}", request.address, request.port),
            request.capacity,
            tx.clone(),
            request.shard_id.clone(),
            request.resources.clone(),
        );
        
        // TODO: In a real implementation, we would spawn a task to forward
        // transfers from rx to the actual solver via network
        tokio::spawn(async move {
            let mut rx = rx;
            while let Some(transfer) = rx.recv().await {
                debug!(
                    transfer_id = %transfer.id,
                    "Transfer ready to send to solver (network forwarding not implemented)"
                );
                // TODO: Send via Anemo RPC to the actual solver
            }
        });
        
        tx
    }
}

/// Registration handler implementation for Validator
pub struct ValidatorRegistrationHandler {
    service: Arc<ValidatorNetworkService>,
}

#[async_trait::async_trait]
impl RegistrationHandler for ValidatorRegistrationHandler {
    async fn register_solver(&self, request: RegisterSolverRequest) -> RegisterSolverResponse {
        info!(
            solver_id = %request.solver_id,
            address = %request.address,
            port = request.port,
            capacity = request.capacity,
            shard_id = ?request.shard_id,
            "Processing solver registration"
        );
        
        // Check if already registered
        if self.service.router_manager.get_solver(&request.solver_id).is_some() {
            warn!(
                solver_id = %request.solver_id,
                "Solver already registered, updating"
            );
        }
        
        // Register the solver
        self.service.register_solver_internal(&request);
        
        info!(
            solver_id = %request.solver_id,
            total_solvers = self.service.router_manager.solver_count(),
            "Solver registered successfully"
        );
        
        RegisterSolverResponse {
            success: true,
            message: "Solver registered successfully".to_string(),
            assigned_id: Some(request.solver_id),
        }
    }
    
    async fn register_validator(&self, request: RegisterValidatorRequest) -> RegisterValidatorResponse {
        info!(
            validator_id = %request.validator_id,
            address = %request.address,
            port = request.port,
            "Processing validator registration"
        );
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let validator_info = ValidatorInfo {
            validator_id: request.validator_id.clone(),
            address: request.address,
            port: request.port,
            status: "online".to_string(),
            registered_at: now,
        };
        
        self.service.validators.write().insert(
            request.validator_id.clone(),
            validator_info,
        );
        
        info!(
            validator_id = %request.validator_id,
            total_validators = self.service.validators.read().len(),
            "Validator registered successfully"
        );
        
        RegisterValidatorResponse {
            success: true,
            message: "Validator registered successfully".to_string(),
        }
    }
    
    async fn unregister(&self, request: UnregisterRequest) -> UnregisterResponse {
        info!(
            node_id = %request.node_id,
            node_type = %request.node_type,
            "Processing unregister request"
        );
        
        match request.node_type {
            NodeType::Solver => {
                self.service.router_manager.unregister_solver(&request.node_id);
                self.service.solver_channels.write().remove(&request.node_id);
                
                UnregisterResponse {
                    success: true,
                    message: "Solver unregistered successfully".to_string(),
                }
            }
            NodeType::Validator => {
                self.service.validators.write().remove(&request.node_id);
                
                UnregisterResponse {
                    success: true,
                    message: "Validator unregistered successfully".to_string(),
                }
            }
        }
    }
    
    async fn heartbeat(&self, request: HeartbeatRequest) -> HeartbeatResponse {
        debug!(
            node_id = %request.node_id,
            current_load = ?request.current_load,
            "Processing heartbeat"
        );
        
        // Update solver load if provided
        if let Some(load) = request.current_load {
            self.service.router_manager.update_solver_load(&request.node_id, load);
        }
        
        let server_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        HeartbeatResponse {
            acknowledged: true,
            server_timestamp,
        }
    }
    
    async fn get_solver_list(&self, request: GetSolverListRequest) -> GetSolverListResponse {
        let solvers = self.service.router_manager.get_all_solvers();
        
        let solver_list: Vec<SolverListItem> = solvers
            .into_iter()
            .filter(|s| {
                // Filter by shard if specified
                if let Some(ref shard_id) = request.shard_id {
                    s.shard_id.as_ref() == Some(shard_id)
                } else {
                    true
                }
            })
            .map(|s| SolverListItem {
                solver_id: s.id,
                address: s.address.split(':').next().unwrap_or("").to_string(),
                port: s.address.split(':').nth(1)
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(0),
                capacity: s.capacity,
                current_load: s.current_load,
                status: format!("{:?}", s.status),
                shard_id: s.shard_id,
            })
            .collect();
        
        GetSolverListResponse { solvers: solver_list }
    }
    
    async fn get_validator_list(&self, _request: GetValidatorListRequest) -> GetValidatorListResponse {
        let validators = self.service.validators.read();
        
        let validator_list: Vec<ValidatorListItem> = validators
            .values()
            .map(|v| ValidatorListItem {
                validator_id: v.validator_id.clone(),
                address: v.address.clone(),
                port: v.port,
                status: v.status.clone(),
            })
            .collect();
        
        GetValidatorListResponse { validators: validator_list }
    }
    
    async fn get_node_status(&self, request: GetNodeStatusRequest) -> GetNodeStatusResponse {
        // Check if it's a solver
        if let Some(solver) = self.service.router_manager.get_solver(&request.node_id) {
            return GetNodeStatusResponse {
                found: true,
                node_id: request.node_id,
                node_type: Some(NodeType::Solver),
                status: Some(format!("{:?}", solver.status)),
                address: Some(solver.address.split(':').next().unwrap_or("").to_string()),
                port: solver.address.split(':').nth(1).and_then(|p| p.parse().ok()),
                uptime_seconds: None, // Not tracked for solvers
            };
        }
        
        // Check if it's a validator
        if let Some(validator) = self.service.validators.read().get(&request.node_id) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            
            return GetNodeStatusResponse {
                found: true,
                node_id: request.node_id,
                node_type: Some(NodeType::Validator),
                status: Some(validator.status.clone()),
                address: Some(validator.address.clone()),
                port: Some(validator.port),
                uptime_seconds: Some(now - validator.registered_at),
            };
        }
        
        // Check if it's this validator
        if request.node_id == self.service.validator_id {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            
            return GetNodeStatusResponse {
                found: true,
                node_id: request.node_id,
                node_type: Some(NodeType::Validator),
                status: Some("online".to_string()),
                address: None,
                port: None,
                uptime_seconds: Some(now - self.service.start_time),
            };
        }
        
        GetNodeStatusResponse {
            found: false,
            node_id: request.node_id,
            node_type: None,
            status: None,
            address: None,
            port: None,
            uptime_seconds: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_register_solver() {
        let router_manager = Arc::new(RouterManager::new());
        let config = NetworkServiceConfig::default();
        let service = Arc::new(ValidatorNetworkService::new(
            "test-validator".to_string(),
            router_manager.clone(),
            config,
        ));
        
        let handler = service.registration_handler();
        
        let request = RegisterSolverRequest {
            solver_id: "solver-1".to_string(),
            address: "127.0.0.1".to_string(),
            port: 9001,
            capacity: 100,
            shard_id: Some("shard-0".to_string()),
            resources: vec!["ETH".to_string()],
            public_key: None,
        };
        
        let response = handler.register_solver(request).await;
        
        assert!(response.success);
        assert_eq!(router_manager.solver_count(), 1);
    }
    
    #[tokio::test]
    async fn test_register_validator() {
        let router_manager = Arc::new(RouterManager::new());
        let config = NetworkServiceConfig::default();
        let service = Arc::new(ValidatorNetworkService::new(
            "test-validator".to_string(),
            router_manager,
            config,
        ));
        
        let handler = service.registration_handler();
        
        let request = RegisterValidatorRequest {
            validator_id: "validator-2".to_string(),
            address: "127.0.0.1".to_string(),
            port: 9002,
            public_key: None,
            stake: Some(1000),
        };
        
        let response = handler.register_validator(request).await;
        
        assert!(response.success);
        assert_eq!(service.validators.read().len(), 1);
    }
    
    #[tokio::test]
    async fn test_get_solver_list() {
        let router_manager = Arc::new(RouterManager::new());
        let config = NetworkServiceConfig::default();
        let service = Arc::new(ValidatorNetworkService::new(
            "test-validator".to_string(),
            router_manager,
            config,
        ));
        
        let handler = service.registration_handler();
        
        // Register two solvers
        handler.register_solver(RegisterSolverRequest {
            solver_id: "solver-1".to_string(),
            address: "127.0.0.1".to_string(),
            port: 9001,
            capacity: 100,
            shard_id: Some("shard-0".to_string()),
            resources: vec![],
            public_key: None,
        }).await;
        
        handler.register_solver(RegisterSolverRequest {
            solver_id: "solver-2".to_string(),
            address: "127.0.0.1".to_string(),
            port: 9002,
            capacity: 200,
            shard_id: Some("shard-1".to_string()),
            resources: vec![],
            public_key: None,
        }).await;
        
        // Get all solvers
        let response = handler.get_solver_list(GetSolverListRequest {
            shard_id: None,
            status_filter: None,
        }).await;
        
        assert_eq!(response.solvers.len(), 2);
        
        // Filter by shard
        let response = handler.get_solver_list(GetSolverListRequest {
            shard_id: Some("shard-0".to_string()),
            status_filter: None,
        }).await;
        
        assert_eq!(response.solvers.len(), 1);
        assert_eq!(response.solvers[0].solver_id, "solver-1");
    }
}

// ============================================
// HTTP Handler Functions (standalone for axum)
// ============================================

async fn http_register_solver(
    State(service): State<Arc<ValidatorNetworkService>>,
    Json(request): Json<RegisterSolverRequest>,
) -> Json<RegisterSolverResponse> {
    let handler = service.registration_handler();
    Json(handler.register_solver(request).await)
}

async fn http_register_validator(
    State(service): State<Arc<ValidatorNetworkService>>,
    Json(request): Json<RegisterValidatorRequest>,
) -> Json<RegisterValidatorResponse> {
    let handler = service.registration_handler();
    Json(handler.register_validator(request).await)
}

async fn http_get_solvers(
    State(service): State<Arc<ValidatorNetworkService>>,
) -> Json<GetSolverListResponse> {
    let handler = service.registration_handler();
    Json(handler.get_solver_list(GetSolverListRequest {
        shard_id: None,
        status_filter: None,
    }).await)
}

async fn http_get_validators(
    State(service): State<Arc<ValidatorNetworkService>>,
) -> Json<GetValidatorListResponse> {
    let handler = service.registration_handler();
    Json(handler.get_validator_list(GetValidatorListRequest {
        status_filter: None,
    }).await)
}

async fn http_submit_transfer(
    State(service): State<Arc<ValidatorNetworkService>>,
    Json(request): Json<SubmitTransferRequest>,
) -> Json<SubmitTransferResponse> {
    Json(service.submit_transfer(request).await)
}

async fn http_get_transfer_status(
    State(service): State<Arc<ValidatorNetworkService>>,
    Json(request): Json<GetTransferStatusRequest>,
) -> Json<GetTransferStatusResponse> {
    Json(service.get_transfer_status(&request.transfer_id))
}

async fn http_heartbeat(
    State(service): State<Arc<ValidatorNetworkService>>,
    Json(request): Json<HeartbeatRequest>,
) -> Json<HeartbeatResponse> {
    let handler = service.registration_handler();
    Json(handler.heartbeat(request).await)
}

async fn http_health(
    State(service): State<Arc<ValidatorNetworkService>>,
) -> Json<serde_json::Value> {
    let uptime = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() - service.start_time;
    
    Json(serde_json::json!({
        "status": "healthy",
        "validator_id": service.validator_id,
        "uptime_seconds": uptime,
        "solver_count": service.router_manager.solver_count(),
        "validator_count": service.validators.read().len(),
    }))
}

