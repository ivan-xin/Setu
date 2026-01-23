//! Setu Solver - Main entry point (solver-tee3 Architecture)
//!
//! ## solver-tee3 Architecture
//!
//! In this architecture, Solver is a **pass-through** node that:
//! 1. Receives fully-prepared `SolverTask` from Validator
//! 2. Passes it to TEE for execution
//! 3. Returns `TeeExecutionResult` back to Validator
//!
//! ## What Solver does NOT do (handled by Validator)
//! - Coin selection
//! - State reads
//! - VLC management
//! - Event creation

use setu_solver::{
    TeeExecutor, TeeExecutionResult,
    SolverNetworkClient, SolverNetworkConfig,
    SolverTask,
    start_server,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn, error, Level};

/// Solver configuration from environment
#[derive(Debug, Clone)]
struct SolverConfig {
    /// Solver ID
    solver_id: String,
    /// Solver listen address
    address: String,
    /// Solver listen port
    port: u16,
    /// Maximum capacity (concurrent tasks)
    capacity: u32,
    /// Shard assignment
    shard_id: Option<String>,
    /// Resource types this solver handles
    resources: Vec<String>,
    /// Validator address
    validator_address: String,
    /// Validator HTTP port
    validator_port: u16,
    /// Heartbeat interval
    heartbeat_interval_secs: u64,
    /// Auto-register on startup
    auto_register: bool,
}

impl SolverConfig {
    fn from_env() -> Self {
        let solver_id = std::env::var("SOLVER_ID")
            .unwrap_or_else(|_| format!("solver-{}", uuid::Uuid::new_v4().to_string()[..8].to_string()));
        
        let address = std::env::var("SOLVER_LISTEN_ADDR")
            .unwrap_or_else(|_| "127.0.0.1".to_string());
        
        let port: u16 = std::env::var("SOLVER_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(9001);
        
        let capacity: u32 = std::env::var("SOLVER_CAPACITY")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100);
        
        let shard_id = std::env::var("SOLVER_SHARD_ID").ok();
        
        let resources: Vec<String> = std::env::var("SOLVER_RESOURCES")
            .ok()
            .map(|s| s.split(',').map(|r| r.trim().to_string()).collect())
            .unwrap_or_default();
        
        let validator_address = std::env::var("VALIDATOR_ADDRESS")
            .unwrap_or_else(|_| "127.0.0.1".to_string());
        
        let validator_port: u16 = std::env::var("VALIDATOR_HTTP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8080);
        
        let heartbeat_interval_secs: u64 = std::env::var("HEARTBEAT_INTERVAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);
        
        let auto_register = std::env::var("AUTO_REGISTER")
            .map(|s| s.to_lowercase() != "false" && s != "0")
            .unwrap_or(true);
        
        Self {
            solver_id,
            address,
            port,
            capacity,
            shard_id,
            resources,
            validator_address,
            validator_port,
            heartbeat_interval_secs,
            auto_register,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .with_target(true)
        .with_thread_ids(true)
        .init();

    // Load configuration
    let config = SolverConfig::from_env();
    
    info!("╔════════════════════════════════════════════════════════════╗");
    info!("║         Setu Solver Node (solver-tee3 Architecture)        ║");
    info!("╠════════════════════════════════════════════════════════════╣");
    info!("║  Solver ID:  {:^44} ║", &config.solver_id);
    info!("║  Address:    {:^44} ║", format!("{}:{}", config.address, config.port));
    info!("║  Capacity:   {:^44} ║", config.capacity);
    info!("║  Shard:      {:^44} ║", config.shard_id.as_deref().unwrap_or("default"));
    info!("║  Validator:  {:^44} ║", format!("{}:{}", config.validator_address, config.validator_port));
    info!("╚════════════════════════════════════════════════════════════╝");

    // Initialize TEE Executor
    let tee_executor = Arc::new(TeeExecutor::new(config.solver_id.clone()));
    
    info!("┌─────────────────────────────────────────────────────────────┐");
    info!("│                  TEE Executor Initialized                   │");
    info!("├─────────────────────────────────────────────────────────────┤");
    info!("│ Enclave:     {:^44} │", tee_executor.enclave_info().enclave_id);
    info!("│ Version:     {:^44} │", tee_executor.enclave_info().version);
    info!("│ Mode:        {:^44} │", if tee_executor.enclave_info().is_simulated { "Mock (Development)" } else { "Production" });
    info!("└─────────────────────────────────────────────────────────────┘");

    // Create network client for Validator communication
    let network_config = SolverNetworkConfig {
        solver_id: config.solver_id.clone(),
        address: config.address.clone(),
        port: config.port,
        capacity: config.capacity,
        shard_id: config.shard_id.clone(),
        resources: config.resources.clone(),
        validator_address: config.validator_address.clone(),
        validator_port: config.validator_port,
        heartbeat_interval_secs: config.heartbeat_interval_secs,
    };
    
    let network_client = Arc::new(SolverNetworkClient::new(network_config));

    // Auto-register with Validator
    if config.auto_register {
        info!("┌─────────────────────────────────────────────────────────────┐");
        info!("│              Registering with Validator                     │");
        info!("└─────────────────────────────────────────────────────────────┘");
        
        match network_client.register().await {
            Ok(response) => {
                if response.success {
                    info!("╔════════════════════════════════════════════════════════════╗");
                    info!("║           ✓ Registration Successful                        ║");
                    info!("╚════════════════════════════════════════════════════════════╝");
                } else {
                    warn!("Registration failed: {}", response.message);
                }
            }
            Err(e) => {
                error!("Failed to register with validator: {}", e);
            }
        }
    }

    // Start heartbeat loop
    let heartbeat_client = network_client.clone();
    let heartbeat_handle = tokio::spawn(async move {
        heartbeat_client.start_heartbeat_loop().await;
    });

    // ============================================
    // HTTP Server for Sync Task Execution (solver-tee3)
    // ============================================
    // 
    // This is the **synchronous HTTP** approach:
    // - Validator sends POST /api/v1/execute-task with SolverTask
    // - Solver executes in TEE synchronously
    // - Solver returns TeeExecutionResult in HTTP response
    // - Validator keeps Event in scope, no pending_tasks needed
    //
    let http_solver_id = config.solver_id.clone();
    let http_executor = tee_executor.clone();
    
    let http_address = config.address.clone();
    let http_port = config.port;
    let http_handle = tokio::spawn(async move {
        info!(
            address = %http_address,
            port = http_port,
            "Starting Solver HTTP server for sync task execution"
        );
        
        if let Err(e) = start_server(&http_address, http_port, http_solver_id, http_executor).await {
            error!("HTTP server failed: {}", e);
        }
    });

    // Log startup complete
    info!("╔════════════════════════════════════════════════════════════╗");
    info!("║              Solver Ready (solver-tee3 Sync HTTP)          ║");
    info!("╠════════════════════════════════════════════════════════════╣");
    info!("║  HTTP Server:  http://{}:{}/api/v1/execute-task {:>11}║", config.address, config.port, "");
    info!("║  Mode:         Synchronous HTTP (no callback)             ║");
    info!("║  Note:         Validator sends task, waits for result     ║");
    info!("╚════════════════════════════════════════════════════════════╝");

    // Wait for shutdown signal
    tokio::select! {
        _ = http_handle => {
            info!("HTTP server stopped");
        }
        _ = heartbeat_handle => {
            info!("Heartbeat loop stopped");
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down...");
            network_client.shutdown();
        }
    }

    info!("Solver shutdown complete");
    Ok(())
}
