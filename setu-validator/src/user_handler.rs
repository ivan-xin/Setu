//! User RPC Handler Implementation
//!
//! This module implements the UserRpcHandler trait for the Validator,
//! providing user-facing RPC services for wallets and DApps.
//!
//! Registration delegates to InfraExecutor for G11-compliant state changes.
//! Balance/account queries read from MerkleStateProvider (StateProvider trait).

use crate::ValidatorNetworkService;
use setu_rpc::{
    UserRpcHandler, RegisterUserRequest, RegisterUserResponse,
    GetAccountRequest, GetAccountResponse, GetBalanceRequest, GetBalanceResponse,
    GetPowerRequest, GetPowerResponse, GetCreditRequest, GetCreditResponse,
    GetCredentialsRequest, GetCredentialsResponse, TransferRequest, TransferResponse,
    CoinBalance, SubmitTransferRequest,
};
use setu_types::registration::UserRegistration;
use setu_vlc::VLCSnapshot;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn, error};

/// User RPC Handler for Validator
pub struct ValidatorUserHandler {
    /// Reference to the network service
    network_service: Arc<ValidatorNetworkService>,
}

impl ValidatorUserHandler {
    /// Create a new user handler
    pub fn new(network_service: Arc<ValidatorNetworkService>) -> Self {
        Self { network_service }
    }

    /// Build error response for register_user
    fn reg_err(message: &str, address: &str) -> RegisterUserResponse {
        RegisterUserResponse {
            success: false,
            message: message.to_string(),
            address: address.to_string(),
            event_id: None,
            initial_flux: 0,
            initial_power: 0,
            initial_credit: 0,
        }
    }
}

#[async_trait::async_trait]
impl UserRpcHandler for ValidatorUserHandler {
    async fn register_user(&self, request: RegisterUserRequest) -> RegisterUserResponse {
        info!(
            address = %request.address,
            subnet_id = ?request.subnet_id,
            is_metamask = %request.nostr_pubkey.is_none(),
            "Processing user registration request"
        );

        // ── Step 1: Validate request ────────────────────────────────
        if request.address.is_empty() {
            return Self::reg_err("Wallet address cannot be empty", &request.address);
        }

        // Accept 66-char Setu native (0x + 64 hex) or 42-char Ethereum (0x + 40 hex)
        if !request.address.starts_with("0x")
            || (request.address.len() != 66 && request.address.len() != 42)
        {
            return Self::reg_err(
                "Invalid address format: expected 0x + 64 hex (Setu) or 0x + 40 hex (Ethereum)",
                &request.address,
            );
        }

        // Nostr-specific validation
        if let Some(ref nostr_pubkey) = request.nostr_pubkey {
            if nostr_pubkey.len() != 32 {
                return Self::reg_err("Nostr public key must be 32 bytes", &request.address);
            }
            if request.signature.is_none() || request.signature.as_ref().unwrap().is_empty() {
                return Self::reg_err("Nostr signature cannot be empty", &request.address);
            }
        }

        // ── Step 2: Signature format check (placeholder for Phase 2 real verification) ──
        if let Some(ref signature) = request.signature {
            if request.nostr_pubkey.is_some() {
                if signature.len() != 64 {
                    return Self::reg_err(
                        "Invalid Nostr signature length (expected 64 bytes)",
                        &request.address,
                    );
                }
            } else if signature.len() != 65 {
                warn!(address = %request.address, "MetaMask signature length != 65, skipping check");
            }
        }

        // ── Step 3: Build VLC snapshot ──────────────────────────────
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let vlc_time = self.network_service.get_vlc_time();
        let mut vlc = setu_vlc::VectorClock::new();
        vlc.increment(self.network_service.validator_id());
        let vlc_snapshot = VLCSnapshot {
            vector_clock: vlc,
            logical_time: vlc_time,
            physical_time: now,
        };

        // ── Step 4: Build UserRegistration ──────────────────────────
        let registration = UserRegistration {
            address: request.address.clone(),
            nostr_pubkey: request.nostr_pubkey.clone(),
            signature: request.signature.clone(),
            message: request.message.clone(),
            timestamp: request.timestamp,
            subnet_id: request.subnet_id.clone(),
            display_name: request.display_name.clone(),
            metadata: request.metadata.clone(),
            invited_by: None,
            invite_code: request.invite_code.clone(),
        };

        // ── Step 5: Delegate to InfraExecutor (路径 B) ──────────────
        // InfraExecutor:
        //   → RuntimeExecutor::execute_user_register()  (G11-compliant "oid:{hex}" state keys)
        //   → apply_state_changes() to MerkleStateProvider
        //   → returns Event with execution_result set
        let event = match self
            .network_service
            .infra_executor()
            .execute_user_register(&registration, vlc_snapshot)
        {
            Ok(event) => event,
            Err(e) => {
                error!(address = %request.address, error = %e, "InfraExecutor user registration failed");
                return Self::reg_err(&format!("Registration failed: {}", e), &request.address);
            }
        };

        let event_id = event.id.clone();

        // ── Step 6: Add event to DAG ────────────────────────────────
        self.network_service.add_event_to_dag(event).await;

        info!(
            address = %request.address,
            event_id = %event_id,
            "User registered successfully (zero initial balance — use Faucet for tokens)"
        );

        RegisterUserResponse {
            success: true,
            message: "User registered successfully".to_string(),
            address: request.address,
            event_id: Some(event_id),
            initial_flux: 0,
            initial_power: 0,
            initial_credit: 0,
        }
    }
    
    async fn get_account(&self, request: GetAccountRequest) -> GetAccountResponse {
        info!(address = %request.address, "Getting account information");

        let coins = self.network_service.state_provider().get_coins_for_address(&request.address);
        let flux_balance: u64 = coins.iter()
            .filter(|c| c.coin_type == "ROOT")
            .map(|c| c.balance)
            .sum();

        GetAccountResponse {
            found: !coins.is_empty(),
            address: request.address,
            flux_balance,
            power: 0,            // Power system not yet implemented
            credit: 0,           // Credit system not yet implemented
            profile: None,       // Profile system not yet implemented
            credential_count: 0, // Credential system not yet implemented
        }
    }
    
    async fn get_balance(&self, request: GetBalanceRequest) -> GetBalanceResponse {
        info!(address = %request.address, "Getting balance");

        let coins = self.network_service.state_provider().get_coins_for_address(&request.address);

        // Aggregate by coin_type
        let mut type_map: std::collections::HashMap<String, (u64, u32)> = std::collections::HashMap::new();
        for c in &coins {
            let entry = type_map.entry(c.coin_type.clone()).or_insert((0, 0));
            entry.0 += c.balance;
            entry.1 += 1;
        }

        // Optional filter by coin_type
        let balances: Vec<CoinBalance> = type_map.into_iter()
            .filter(|(ct, _)| {
                request.coin_type.as_ref().map_or(true, |filter| ct == filter)
            })
            .map(|(coin_type, (balance, coin_count))| CoinBalance {
                coin_type,
                balance,
                coin_count,
            })
            .collect();

        let total_balance = balances.iter().map(|b| b.balance).sum();

        GetBalanceResponse {
            found: !coins.is_empty(),
            address: request.address,
            balances,
            total_balance,
        }
    }
    
    async fn get_power(&self, request: GetPowerRequest) -> GetPowerResponse {
        // Power system not yet implemented — return zeros
        GetPowerResponse {
            found: false,
            address: request.address,
            power: 0,
            rank: None,
            recent_changes: vec![],
        }
    }
    
    async fn get_credit(&self, request: GetCreditRequest) -> GetCreditResponse {
        // Credit system not yet implemented — return zeros
        GetCreditResponse {
            found: false,
            address: request.address,
            credit: 0,
            level: None,
            recent_changes: vec![],
        }
    }
    
    async fn get_credentials(&self, request: GetCredentialsRequest) -> GetCredentialsResponse {
        // Credential system not yet implemented — return empty
        GetCredentialsResponse {
            found: false,
            address: request.address,
            credentials: vec![],
            valid_count: 0,
        }
    }
    
    async fn transfer(&self, request: TransferRequest) -> TransferResponse {
        info!(
            from = %request.from,
            to = %request.to,
            amount = request.amount,
            "Processing transfer request"
        );
        
        // Convert to SubmitTransferRequest
        let submit_request = SubmitTransferRequest {
            from: request.from,
            to: request.to,
            amount: request.amount,
            transfer_type: request.coin_type.unwrap_or_else(|| "flux".to_string()),
            resources: vec![],
            preferred_solver: None,
            shard_id: None,
            subnet_id: None,
        };
        
        // Use existing transfer submission logic
        let response = self.network_service.submit_transfer(submit_request).await;
        
        TransferResponse {
            success: response.success,
            message: response.message,
            event_id: response.transfer_id,
            estimated_confirmation: Some(2), // ~2 seconds
        }
    }
}

