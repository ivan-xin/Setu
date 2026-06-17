//! HTTP Client for benchmark
//!
//! Wraps reqwest client with retry logic and metrics collection.

use crate::metrics::RequestMetrics;
use anyhow::Result;
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Convert a human-readable account name (e.g., "alice", "user_001") into
/// the canonical `0x`-prefixed hex address that the Validator expects.
///
/// This replicates the logic of `Address::from_str_id` in `setu-types`:
///   blake3::hash(name.as_bytes()) → 32-byte digest → "0x" + hex
///
/// If the input is already a valid hex address (64 hex chars with optional "0x" prefix),
/// it is returned as-is (normalized with "0x" prefix).
pub fn name_to_hex_address(name: &str) -> String {
    let stripped = name.strip_prefix("0x").unwrap_or(name);
    if stripped.len() == 64 && stripped.chars().all(|c| c.is_ascii_hexdigit()) {
        // Already a hex address – normalize with 0x prefix
        return format!("0x{}", stripped);
    }
    let hash = blake3::hash(name.as_bytes());
    format!("0x{}", hex::encode(hash.as_bytes()))
}

/// Load seed account addresses from genesis.json.
///
/// Returns a Vec of `(name_or_label, hex_address)` pairs.
/// The addresses in genesis.json are the authoritative source —
/// they may be derived from real keypairs and do NOT necessarily match
/// `blake3::hash(name)`.
pub fn load_seed_addresses_from_genesis(genesis_path: &str) -> Result<Vec<String>> {
    use setu_types::GenesisConfig;
    let config = GenesisConfig::load(genesis_path)
        .map_err(|e| anyhow::anyhow!("Failed to load genesis file '{}': {}", genesis_path, e))?;
    
    let addresses: Vec<String> = config.accounts.iter()
        .map(|a| a.address.clone())
        .collect();
    
    if addresses.is_empty() {
        anyhow::bail!("No accounts found in genesis file '{}'", genesis_path);
    }
    
    Ok(addresses)
}

/// Transfer request for benchmark
#[derive(Debug, Clone, Serialize)]
pub struct BenchTransferRequest {
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub transfer_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_solver: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shard_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subnet_id: Option<String>,
    #[serde(default)]
    pub resources: Vec<String>,
}

/// Transfer response
#[derive(Debug, Clone, Deserialize)]
pub struct BenchTransferResponse {
    pub success: bool,
    pub message: String,
    pub transfer_id: Option<String>,
    #[allow(dead_code)]
    pub solver_id: Option<String>,
}

/// Batch transfer request
#[derive(Debug, Clone, Serialize)]
pub struct BenchBatchRequest {
    pub transfers: Vec<BenchTransferRequest>,
}

/// Batch transfer result for a single transfer
#[derive(Debug, Clone, Deserialize)]
pub struct BenchBatchTransferResult {
    pub index: usize,
    pub success: bool,
    pub transfer_id: Option<String>,
    pub solver_id: Option<String>,
    pub error: Option<String>,
}

/// Batch transfer response
#[derive(Debug, Clone, Deserialize)]
pub struct BenchBatchResponse {
    pub success: bool,
    pub message: String,
    pub submitted: usize,
    pub failed: usize,
    pub results: Vec<BenchBatchTransferResult>,
    #[allow(dead_code)]
    pub stats: Option<BenchBatchStats>,
}

/// Batch preparation statistics
#[derive(Debug, Clone, Deserialize)]
pub struct BenchBatchStats {
    #[allow(dead_code)]
    pub total_transfers: usize,
    #[allow(dead_code)]
    pub unique_sender_subnet_pairs: usize,
    #[allow(dead_code)]
    pub coins_selected: usize,
    #[allow(dead_code)]
    pub same_sender_conflicts: usize,
}

/// Benchmark HTTP client
pub struct BenchClient {
    client: Client,
    base_url: String,
    timeout: Duration,
}

impl BenchClient {
    fn with_raw_transfer_token(builder: RequestBuilder) -> RequestBuilder {
        match std::env::var("SETU_RAW_TRANSFER_API_TOKEN") {
            Ok(token) if !token.is_empty() => builder.header("X-Setu-Admin-Token", token),
            _ => builder,
        }
    }

    /// Create a new benchmark client
    pub fn new(base_url: String, timeout_secs: u64, keep_alive: bool) -> Result<Self> {
        let mut builder = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .pool_max_idle_per_host(100)
            .pool_idle_timeout(Duration::from_secs(30));

        if !keep_alive {
            builder = builder.pool_max_idle_per_host(0);
        }

        let client = builder.build()?;

        Ok(Self {
            client,
            base_url,
            timeout: Duration::from_secs(timeout_secs),
        })
    }

    /// Submit a transfer and measure latency
    pub async fn submit_transfer(&self, request: BenchTransferRequest) -> RequestMetrics {
        let url = format!("{}/api/v1/transfer", self.base_url);
        let start = Instant::now();

        let result = Self::with_raw_transfer_token(self.client.post(&url).json(&request))
            .send()
            .await;

        let latency = start.elapsed();

        match result {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    match response.json::<BenchTransferResponse>().await {
                        Ok(resp) => {
                            if resp.success {
                                debug!(
                                    transfer_id = ?resp.transfer_id,
                                    latency_ms = latency.as_millis(),
                                    "Transfer succeeded"
                                );
                                RequestMetrics::success(latency)
                            } else {
                                warn!(message = %resp.message, "Transfer failed (application error)");
                                RequestMetrics::failure(latency, resp.message)
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to parse response");
                            RequestMetrics::failure(latency, format!("Parse error: {}", e))
                        }
                    }
                } else {
                    let body = response.text().await.unwrap_or_default();
                    warn!(status = %status, body = %body, "HTTP error");
                    RequestMetrics::failure(latency, format!("HTTP {}: {}", status, body))
                }
            }
            Err(e) => {
                if e.is_timeout() {
                    warn!(timeout_ms = self.timeout.as_millis(), "Request timeout");
                    RequestMetrics::timeout(latency)
                } else if e.is_connect() {
                    warn!(error = %e, "Connection error");
                    RequestMetrics::failure(latency, format!("Connection error: {}", e))
                } else {
                    warn!(error = %e, "Request error");
                    RequestMetrics::failure(latency, format!("Request error: {}", e))
                }
            }
        }
    }

    /// Health check
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/api/v1/status", self.base_url);
        let response = self.client.get(&url).send().await?;
        Ok(response.status().is_success())
    }

    /// Submit a batch of transfers and measure latency
    /// 
    /// Returns a vector of RequestMetrics, one for each transfer in the batch.
    /// The latency is the total batch request latency divided by batch size.
    pub async fn submit_transfers_batch(&self, requests: Vec<BenchTransferRequest>) -> Vec<RequestMetrics> {
        let batch_size = requests.len();
        if batch_size == 0 {
            return vec![];
        }

        let url = format!("{}/api/v1/transfers/batch", self.base_url);
        let start = Instant::now();

        let batch_request = BenchBatchRequest { transfers: requests };

        let result = Self::with_raw_transfer_token(self.client.post(&url).json(&batch_request))
            .send()
            .await;

        let total_latency = start.elapsed();
        // Approximate per-transfer latency (batch amortizes overhead)
        let per_transfer_latency = total_latency / batch_size as u32;

        match result {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    match response.json::<BenchBatchResponse>().await {
                        Ok(resp) => {
                            debug!(
                                submitted = resp.submitted,
                                failed = resp.failed,
                                total_latency_ms = total_latency.as_millis(),
                                "Batch transfer completed"
                            );
                            
                            // Convert batch results to individual metrics
                            resp.results
                                .iter()
                                .map(|r| {
                                    if r.success {
                                        RequestMetrics::success(per_transfer_latency)
                                    } else {
                                        RequestMetrics::failure(
                                            per_transfer_latency,
                                            r.error.clone().unwrap_or_else(|| "Unknown error".to_string()),
                                        )
                                    }
                                })
                                .collect()
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to parse batch response");
                            vec![RequestMetrics::failure(total_latency, format!("Parse error: {}", e)); batch_size]
                        }
                    }
                } else {
                    let body = response.text().await.unwrap_or_default();
                    warn!(status = %status, body = %body, "HTTP error on batch request");
                    vec![RequestMetrics::failure(total_latency, format!("HTTP {}: {}", status, body)); batch_size]
                }
            }
            Err(e) => {
                if e.is_timeout() {
                    warn!(timeout_ms = self.timeout.as_millis(), "Batch request timeout");
                    vec![RequestMetrics::timeout(total_latency); batch_size]
                } else {
                    warn!(error = %e, "Batch request error");
                    vec![RequestMetrics::failure(total_latency, format!("Request error: {}", e)); batch_size]
                }
            }
        }
    }
}

/// Generate a random transfer request
/// 
/// NOTE: Each request gets a unique resource key based on seq to ensure
/// consistent hash routing distributes requests evenly across solvers.
pub fn generate_transfer(
    sender_prefix: &str,
    receiver_prefix: &str,
    amount: u64,
    seq: u64,
    subnet_id: Option<String>,
) -> BenchTransferRequest {
    BenchTransferRequest {
        from: name_to_hex_address(&format!("{}_{}", sender_prefix, seq % 1000)),
        to: name_to_hex_address(&format!("{}_{}", receiver_prefix, (seq + 500) % 1000)),
        amount,
        transfer_type: "setu".to_string(),
        preferred_solver: None,
        shard_id: None,
        subnet_id,
        // Use unique resource key per request to ensure even distribution
        // across solvers via consistent hash routing
        resources: vec![format!("bench_resource_{}", seq)],
    }
}

/// Generate a transfer using test accounts
/// 
/// Test accounts are created in two tiers:
/// 1. Seed accounts: alice, bob, charlie (always available, high balance)
/// 2. User accounts: user_001, user_002, ... (created via --init-accounts)
/// 
/// If `num_test_accounts` is specified, uses that many user accounts.
/// Otherwise, falls back to using only the 3 seed accounts.
/// 
/// ## Multi-Coin Object Model
/// 
/// Setu uses a multi-coin object model where each account can own multiple
/// coin objects of the same type. Each coin can be reserved independently
/// for parallel transfers. The number of coins per account determines
/// the per-sender concurrency limit.
/// 
/// With `--coins-per-account N`, each account has N coin objects.
/// Effective per-sender parallelism = min(coins_per_account, concurrent_transfers_from_sender)
/// 
/// For best results:
/// - Set coins_per_account >= concurrency / init_accounts
/// - More accounts + more coins = higher aggregate concurrency
/// 
/// ## NOTE on Solver Distribution
/// 
/// Each request gets a unique resource key based on seq to ensure
/// consistent hash routing distributes requests evenly across solvers.
/// Without this, all requests from the same 'from' address would route
/// to the same solver, causing severe load imbalance in multi-solver tests.
#[allow(dead_code)]
pub fn generate_transfer_with_test_accounts(amount: u64, seq: u64, seed_addresses: &[String]) -> BenchTransferRequest {
    generate_transfer_with_n_accounts(amount, seq, None, seed_addresses, None)
}

/// Generate a transfer using a specified number of test accounts
/// 
/// - If `num_accounts` is None or 0, uses only seed accounts from genesis
/// - Otherwise, uses user_001 to user_N (must be initialized via --init-accounts first)
///
/// `seed_addresses` are the authoritative hex addresses loaded from genesis.json.
pub fn generate_transfer_with_n_accounts(amount: u64, seq: u64, num_accounts: Option<u64>, seed_addresses: &[String], subnet_id: Option<String>) -> BenchTransferRequest {
    // Build account list based on configuration
    // All names are converted to canonical hex addresses for production Validator compatibility.
    let accounts: Vec<String> = match num_accounts {
        Some(n) if n > 0 => {
            // Use initialized user accounts (user_001 to user_N) → hex
            (1..=n).map(|i| name_to_hex_address(&format!("user_{:03}", i))).collect()
        }
        _ => {
            // Fall back to seed accounts from genesis
            seed_addresses.to_vec()
        }
    };
    
    let num = accounts.len();
    
    // Use different indices for sender and receiver to ensure they're different
    let sender_idx = seq as usize % num;
    let receiver_idx = (seq as usize + 1 + (seq as usize / num)) % num;
    
    // Ensure sender != receiver
    let receiver_idx = if receiver_idx == sender_idx {
        (receiver_idx + 1) % num
    } else {
        receiver_idx
    };
    
    BenchTransferRequest {
        from: accounts[sender_idx].clone(),
        to: accounts[receiver_idx].clone(),
        amount,
        transfer_type: "setu".to_string(),
        preferred_solver: None,
        shard_id: None,
        subnet_id,
        // Use unique resource key per request to ensure even distribution
        // across solvers via consistent hash routing
        resources: vec![format!("bench_resource_{}", seq)],
    }
}

/// Response for balance query
#[derive(Debug, Deserialize)]
pub struct GetBalanceResponse {
    pub account: String,
    pub balance: u128,
    pub exists: bool,
}

/// Move call request for benchmark (mirrors api::types::MoveCallRequest)
#[derive(Debug, Clone, Serialize)]
pub struct BenchMoveCallRequest {
    pub sender: String,
    pub package: String,
    pub module: String,
    pub function: String,
    #[serde(default)]
    pub type_args: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub input_object_ids: Vec<String>,
    #[serde(default)]
    pub mutable_indices: Vec<usize>,
    #[serde(default)]
    pub consumed_indices: Vec<usize>,
    pub needs_tx_context: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subnet_id: Option<String>,
}

/// Move call response for benchmark (mirrors api::types::MoveCallResponse)
#[derive(Debug, Deserialize)]
pub struct BenchMoveCallResponse {
    pub success: bool,
    pub event_id: String,
    pub state_changes: usize,
    pub error: Option<String>,
}

impl BenchClient {
    /// Query account balance
    /// 
    /// Returns Some(balance) if the account exists, None otherwise.
    /// This can be used to check if a transfer has been applied to state.
    pub async fn get_balance(&self, account: &str) -> Option<u64> {
        let hex_account = name_to_hex_address(account);
        let url = format!("{}/api/v1/state/balance/{}", self.base_url, hex_account);
        
        match self.client.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                match response.json::<GetBalanceResponse>().await {
                    Ok(resp) if resp.exists => Some(resp.balance as u64),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Submit a Move call and measure latency
    pub async fn submit_move_call(&self, request: BenchMoveCallRequest) -> RequestMetrics {
        let url = format!("{}/api/v1/move/call", self.base_url);
        let start = Instant::now();

        let result = self.client.post(&url).json(&request).send().await;
        let latency = start.elapsed();

        match result {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    match response.json::<BenchMoveCallResponse>().await {
                        Ok(resp) => {
                            if resp.success {
                                debug!(
                                    event_id = %resp.event_id,
                                    state_changes = resp.state_changes,
                                    latency_ms = latency.as_millis(),
                                    "Move call succeeded"
                                );
                                RequestMetrics::success(latency)
                            } else {
                                warn!(error = ?resp.error, "Move call failed (application error)");
                                RequestMetrics::failure(
                                    latency,
                                    resp.error.unwrap_or_else(|| "Unknown error".to_string()),
                                )
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to parse Move call response");
                            RequestMetrics::failure(latency, format!("Parse error: {}", e))
                        }
                    }
                } else {
                    let body = response.text().await.unwrap_or_default();
                    warn!(status = %status, body = %body, "HTTP error on Move call");
                    RequestMetrics::failure(latency, format!("HTTP {}: {}", status, body))
                }
            }
            Err(e) => {
                if e.is_timeout() {
                    warn!(timeout_ms = self.timeout.as_millis(), "Move call request timeout");
                    RequestMetrics::timeout(latency)
                } else {
                    warn!(error = %e, "Move call request error");
                    RequestMetrics::failure(latency, format!("Request error: {}", e))
                }
            }
        }
    }
}

// =============================================================================
// Multi-target funding gate + account partitioning
// (docs/feat/benchmark-multitarget-funding-gate/design.md — D1/D2)
// =============================================================================

/// Compute `(sender_idx, receiver_idx)` — 0-based account indices — for a
/// partitioned transfer. Validator `client_idx` (of `num_clients`) draws BOTH
/// sender and receiver ONLY from its own contiguous block of accounts
/// (intra-block closed economy, D2), so a coin is never reserved on more than
/// one validator and total coins per block are conserved across a run.
///
/// `local_seq` advances the in-block selection; it varies across retries to pick
/// a different in-block account while `client_idx` keeps the block fixed (so a
/// retry can never spill into another validator's block — design F5/R1-4).
///
/// Single-target (`num_clients <= 1`) reduces to the legacy
/// `generate_transfer_with_n_accounts` selection exactly when `local_seq == seq`
/// (block = all accounts). Leftover accounts when `num_accounts % num_clients != 0`
/// are simply never selected (D5).
pub fn partition_indices(
    num_accounts: usize,
    num_clients: usize,
    client_idx: usize,
    local_seq: u64,
) -> (usize, usize) {
    debug_assert!(num_accounts > 0, "num_accounts must be > 0");
    let num_clients = num_clients.max(1);
    let client_idx = client_idx % num_clients;
    // Block size (accounts per validator). Round down; leftover accounts when
    // num_accounts % num_clients != 0 are never selected (D5). max(1) guards the
    // degenerate num_accounts < num_clients case (config should prevent it).
    let k = (num_accounts / num_clients).max(1);
    let block_start = client_idx * k;

    let local = local_seq as usize;
    let s = local % k;
    let mut r = (local + 1 + (local / k)) % k;
    if r == s {
        r = (r + 1) % k;
    }
    (block_start + s, block_start + r)
}

/// Generate a partitioned transfer request (multi-target path, D2/D3).
pub fn generate_transfer_partitioned(
    amount: u64,
    num_accounts: usize,
    num_clients: usize,
    client_idx: usize,
    local_seq: u64,
    subnet_id: Option<String>,
) -> BenchTransferRequest {
    let (sender_idx, receiver_idx) =
        partition_indices(num_accounts, num_clients, client_idx, local_seq);
    let from = name_to_hex_address(&format!("user_{:03}", sender_idx + 1));
    let to = name_to_hex_address(&format!("user_{:03}", receiver_idx + 1));
    BenchTransferRequest {
        from,
        to,
        amount,
        transfer_type: "setu".to_string(),
        preferred_solver: None,
        shard_id: None,
        subnet_id,
        resources: vec![format!("bench_resource_{}_{}", client_idx, local_seq)],
    }
}

/// Account indices (1-based, for `user_{:03}` names) to sample in the funding gate
/// (D1). `sample == 0` → all accounts. Otherwise first + last + evenly-strided
/// middle accounts, deduped and sorted (so the returned length may be `<= sample`).
pub fn funding_gate_sample(num_accounts: u64, sample: u64) -> Vec<u64> {
    if num_accounts == 0 {
        return vec![];
    }
    if sample == 0 || sample >= num_accounts {
        return (1..=num_accounts).collect();
    }
    if sample == 1 {
        // The last account is the strictest single probe (funded last).
        return vec![num_accounts];
    }
    let mut idxs = std::collections::BTreeSet::new();
    idxs.insert(1);
    idxs.insert(num_accounts);
    // Distribute the remaining (sample - 2) probes evenly across the middle.
    let remaining = sample - 2;
    let span = num_accounts - 1; // positions 1..num_accounts
    for j in 1..=remaining {
        let pos = 1 + (span * j) / (remaining + 1);
        idxs.insert(pos);
    }
    idxs.into_iter().collect()
}

/// Funding-gate readiness predicate (D1). `balances[client_idx][sample_pos]` is the
/// polled balance (or `None` if absent). Returns `Ok(())` iff every entry is
/// `Some(>0)`; otherwise `Err((client_idx, account_index))` for the first not-ready
/// pair (`account_index` is the 1-based user index from `sample`).
pub fn gate_ready(
    balances: &[Vec<Option<u64>>],
    sample: &[u64],
) -> Result<(), (usize, u64)> {
    for (ci, row) in balances.iter().enumerate() {
        for (si, bal) in row.iter().enumerate() {
            match bal {
                Some(b) if *b > 0 => {}
                _ => return Err((ci, sample.get(si).copied().unwrap_or(0))),
            }
        }
    }
    Ok(())
}

/// Emit a JSON array of `count` pre-funded genesis account entries (user_001..user_count),
/// using the SAME `name_to_hex_address` derivation the load path uses — so the accounts
/// baked into genesis are exactly the ones the benchmark will transfer between. Splice the
/// output into genesis `accounts`, then run with `--skip-funding`.
pub fn emit_genesis_accounts_json(count: u64, balance: u64, coins_per_account: u64) -> String {
    let entries: Vec<String> = (1..=count)
        .map(|i| {
            let name = format!("user_{:03}", i);
            let addr = name_to_hex_address(&name);
            format!(
                "    {{ \"address\": \"{}\", \"name\": \"{}\", \"balance\": {}, \"coins_per_account\": {} }}",
                addr, name, balance, coins_per_account
            )
        })
        .collect();
    format!("[\n{}\n]", entries.join(",\n"))
}

/// Generate a Move call request from benchmark config
pub fn generate_move_call(
    config: &crate::config::BenchmarkConfig,
    seq: u64,
    seed_addresses: &[String],
) -> BenchMoveCallRequest {
    // Use round-robin sender across seed accounts
    let sender = &seed_addresses[seq as usize % seed_addresses.len()];

    BenchMoveCallRequest {
        sender: sender.clone(),
        package: config.move_package.clone(),
        module: config.move_module.clone(),
        function: config.move_function.clone(),
        type_args: config.get_move_type_args(),
        args: config.get_move_args(),
        input_object_ids: vec![],
        mutable_indices: vec![],
        consumed_indices: vec![],
        needs_tx_context: true,
        subnet_id: None,
    }
}

#[cfg(test)]
mod partition_tests {
    use super::{funding_gate_sample, gate_ready, partition_indices};

    /// Legacy selection formula from `generate_transfer_with_n_accounts` (client.rs),
    /// reproduced here as the oracle for single-target equivalence (T2).
    fn legacy_indices(seq: u64, num: usize) -> (usize, usize) {
        let s = seq as usize % num;
        let mut r = (seq as usize + 1 + (seq as usize / num)) % num;
        if r == s {
            r = (r + 1) % num;
        }
        (s, r)
    }

    /// T1 — sender & receiver always land inside the validator's own block.
    #[test]
    fn t1_sender_receiver_in_block() {
        let (na, nc) = (198usize, 3usize);
        let k = na / nc; // 66
        for client_idx in 0..nc {
            let (lo, hi) = (client_idx * k, client_idx * k + k);
            for seq in 0..500u64 {
                let (s, r) = partition_indices(na, nc, client_idx, seq);
                assert!((lo..hi).contains(&s), "sender {} not in [{},{})", s, lo, hi);
                assert!((lo..hi).contains(&r), "receiver {} not in [{},{})", r, lo, hi);
            }
        }
    }

    /// T2 — single-target (num_clients=1) matches the legacy formula byte-for-byte.
    #[test]
    fn t2_single_target_matches_legacy() {
        let num = 200usize;
        for seq in 0..1000u64 {
            assert_eq!(
                partition_indices(num, 1, 0, seq),
                legacy_indices(seq, num),
                "mismatch at seq {}",
                seq
            );
        }
    }

    /// T3 — sender != receiver for any block size >= 2.
    #[test]
    fn t3_sender_ne_receiver() {
        for &(na, nc) in &[(198usize, 3usize), (200, 1), (120, 4), (99, 3)] {
            for client_idx in 0..nc {
                for seq in 0..300u64 {
                    let (s, r) = partition_indices(na, nc, client_idx, seq);
                    assert_ne!(s, r, "na={} nc={} c={} seq={}", na, nc, client_idx, seq);
                }
            }
        }
    }

    /// T4 — remainder alignment: (200,3) → k=66, blocks [0,66)[66,132)[132,198);
    /// leftover accounts 198,199 (0-based) are never selected; never out of bounds.
    #[test]
    fn t4_remainder_alignment() {
        let (na, nc) = (200usize, 3usize);
        let k = na / nc;
        assert_eq!(k, 66);
        for client_idx in 0..nc {
            let (lo, hi) = (client_idx * k, client_idx * k + k);
            assert!(hi <= 198, "block end {} leaks into leftover region", hi);
            for seq in 0..400u64 {
                let (s, r) = partition_indices(na, nc, client_idx, seq);
                assert!(s < na && r < na, "out of bounds s={} r={}", s, r);
                assert!((lo..hi).contains(&s) && (lo..hi).contains(&r));
            }
        }
    }

    /// T5 — funding-gate sample set: includes first+last, deduped/sorted, capped.
    #[test]
    fn t5_funding_gate_sample() {
        let s = funding_gate_sample(200, 8);
        assert!(s.contains(&1) && s.contains(&200), "must include first & last");
        assert!(s.len() <= 8 && s.len() >= 2);
        assert!(s.windows(2).all(|w| w[0] < w[1]), "must be strictly sorted+deduped");
        assert!(s.iter().all(|&x| (1..=200).contains(&x)));
        // sample == 0 → all
        assert_eq!(funding_gate_sample(50, 0), (1..=50).collect::<Vec<_>>());
        // sample >= total → all
        assert_eq!(funding_gate_sample(5, 8), vec![1, 2, 3, 4, 5]);
        // empty
        assert!(funding_gate_sample(0, 8).is_empty());
    }

    /// T6 — readiness predicate: all Some(>0) ⇒ Ok; first not-ready ⇒ Err((client,acct)).
    #[test]
    fn t6_gate_ready_predicate() {
        let sample = vec![1u64, 50, 100];
        let ok = vec![
            vec![Some(10), Some(10), Some(10)],
            vec![Some(5), Some(5), Some(5)],
        ];
        assert!(gate_ready(&ok, &sample).is_ok());
        // client 1, account 50 absent
        let missing = vec![
            vec![Some(10), Some(10), Some(10)],
            vec![Some(5), None, Some(5)],
        ];
        assert_eq!(gate_ready(&missing, &sample), Err((1, 50)));
        // zero balance counts as not-ready (client 0, account 1)
        let zero = vec![vec![Some(0), Some(10), Some(10)]];
        assert_eq!(gate_ready(&zero, &sample), Err((0, 1)));
    }
}
