//! Benchmark configuration

use clap::{Parser, ValueEnum};
use tracing::info;

/// Benchmark mode
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum BenchmarkMode {
    /// Burst mode: send all transactions as fast as possible
    #[default]
    Burst,
    /// Sustained mode: maintain a target TPS for a duration
    Sustained,
    /// Ramp mode: gradually increase load to find max TPS
    Ramp,
}

/// Workload type
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum WorkloadType {
    /// Native transfer workload (default)
    #[default]
    Transfer,
    /// Move VM call workload (requires a deployed contract)
    MoveCall,
}

/// Setu TPS Benchmark Configuration
#[derive(Parser, Debug, Clone)]
#[command(name = "setu-benchmark")]
#[command(about = "TPS Benchmark tool for Setu network", long_about = None)]
pub struct BenchmarkConfig {
    /// Validator HTTP API URL (single validator mode)
    #[arg(short = 'u', long, default_value = "http://127.0.0.1:8080")]
    pub validator_url: String,

    /// Comma-separated list of validator URLs for multi-validator load distribution.
    /// When set, transactions are distributed round-robin across these validators.
    /// Overrides --validator-url.
    #[arg(long, default_value = "")]
    pub validator_urls: String,

    /// Comma-separated list of subnet IDs to assign to transactions.
    /// Transactions cycle through these subnets in round-robin fashion.
    /// Example: "subnet-app-1,subnet-app-2,subnet-app-3"
    #[arg(long, default_value = "")]
    pub subnets: String,

    /// Benchmark mode
    #[arg(short = 'm', long, value_enum, default_value = "burst")]
    pub mode: BenchmarkMode,

    /// Total number of transactions to send (burst mode)
    #[arg(short = 't', long, default_value = "1000")]
    pub total: u64,

    /// Number of concurrent workers
    #[arg(short = 'c', long, default_value = "10")]
    pub concurrency: u64,

    /// Duration in seconds (sustained/ramp mode)
    #[arg(short = 'd', long, default_value = "60")]
    pub duration: u64,

    /// Target TPS (sustained mode)
    #[arg(long, default_value = "100")]
    pub target_tps: u64,

    /// Starting TPS for ramp mode
    #[arg(long, default_value = "10")]
    pub ramp_start: u64,

    /// TPS increment per step in ramp mode
    #[arg(long, default_value = "10")]
    pub ramp_step: u64,

    /// Duration of each step in ramp mode (seconds)
    #[arg(long, default_value = "10")]
    pub ramp_step_duration: u64,

    /// Number of warmup transactions before measurement
    #[arg(long, default_value = "100")]
    pub warmup: u64,

    /// Enable detailed per-request logging
    #[arg(long, default_value = "false")]
    pub verbose: bool,

    /// Output format (text, json, csv)
    #[arg(long, default_value = "text")]
    pub output: String,

    /// Sender address prefix (will be randomized)
    #[arg(long, default_value = "bench_sender")]
    pub sender_prefix: String,

    /// Receiver address prefix (will be randomized)
    #[arg(long, default_value = "bench_receiver")]
    pub receiver_prefix: String,

    /// Transfer amount per transaction
    #[arg(long, default_value = "100")]
    pub amount: u64,

    /// HTTP request timeout in seconds
    #[arg(long, default_value = "30")]
    pub timeout: u64,

    /// Enable keep-alive connections
    #[arg(long, default_value = "true")]
    pub keep_alive: bool,

    /// Use pre-initialized test accounts (alice, bob, charlie) instead of random addresses
    #[arg(long, default_value = "false")]
    pub use_test_accounts: bool,

    /// Number of test accounts to initialize before benchmark
    /// 
    /// When > 0, transfers funds from seed accounts (alice, bob, charlie) to
    /// create user_001, user_002, ... user_N test accounts.
    /// Each test account receives `init_account_balance` tokens.
    /// 
    /// Seed accounts are pre-sharded at genesis with multiple coins
    /// (default 5 per genesis.json `coins_per_account`), so up to
    /// 3 seeds × 5 coins = 15 accounts can be initialized in parallel
    /// per round. More seed coins → faster init.
    /// 
    /// This enables high-concurrency testing without requiring Validator
    /// to pre-initialize all test accounts.
    #[arg(long, default_value = "0")]
    pub init_accounts: u64,

    /// Balance to transfer to each initialized test account
    #[arg(long, default_value = "100000")]
    pub init_account_balance: u64,

    /// Number of coin objects to create per account (for concurrency)
    /// 
    /// Setu uses a multi-coin object model where each account can own
    /// multiple coin objects of the same type. Each coin can be reserved
    /// independently, enabling parallel transfers from the same sender.
    /// 
    /// During account initialization, the balance is split into N coin
    /// objects. More coins = higher per-account concurrency.
    /// 
    /// Recommended: set to concurrency / init_accounts * 2 or higher.
    #[arg(long, default_value = "5")]
    pub coins_per_account: u64,

    /// Path to genesis.json file (used to read seed account addresses)
    #[arg(long, default_value = "genesis.json")]
    pub genesis_file: String,

    /// Use batch API instead of single transfer API
    #[arg(long, default_value = "false")]
    pub use_batch: bool,

    /// Batch size (number of transfers per batch request)
    #[arg(long, default_value = "50")]
    pub batch_size: u64,

    // ── Move call workload options ──────────────────────────

    /// Workload type: transfer (default) or move-call
    #[arg(long, value_enum, default_value = "transfer")]
    pub workload: WorkloadType,

    /// Move package address (hex, e.g. "0xcafe"). Required for move-call workload.
    #[arg(long, default_value = "")]
    pub move_package: String,

    /// Move module name (e.g. "counter"). Required for move-call workload.
    #[arg(long, default_value = "")]
    pub move_module: String,

    /// Move function name (e.g. "create"). Required for move-call workload.
    #[arg(long, default_value = "")]
    pub move_function: String,

    /// Move type arguments (comma-separated, e.g. "0x1::setu::SETU")
    #[arg(long, default_value = "")]
    pub move_type_args: String,

    /// Move pure arguments (comma-separated hex-encoded BCS values)
    #[arg(long, default_value = "")]
    pub move_args: String,

    // ── M0 pipeline profiling (docs/feat/m0-pipeline-baseline/) ──────────────
    /// Pull the validator's per-stage M0 timing report after the run.
    ///
    /// Requires the validator to be built with `--features m0-profiling`.
    /// Resets the measurement window before the load, then fetches and renders
    /// the per-stage p50/p95/p99 breakdown from `{validator_url}/api/v1/m0/report`.
    /// Run at several `--concurrency` levels to build the saturation sweep (design D4).
    #[arg(long, default_value = "false")]
    pub m0: bool,

    // ── Multi-target reliability (docs/feat/benchmark-multitarget-funding-gate/) ──
    /// Funding gate (D1): accounts sampled PER VALIDATOR before load starts.
    /// A number polls first+last+evenly-strided middle accounts; "all" polls every
    /// account. The gate blocks until every sampled account is funded on EVERY
    /// validator — so round-robin load never hits a validator that hasn't yet
    /// applied the funding state ("No coins"). Default 8.
    #[arg(long, default_value = "8")]
    pub funding_gate_accounts: String,

    /// Funding gate timeout in seconds (default 60).
    #[arg(long, default_value = "60")]
    pub funding_gate_timeout_secs: u64,

    /// If the funding gate times out, continue anyway. Default false = abort
    /// (fail loud, since a timed-out gate yields misleading TPS).
    #[arg(long, default_value = "false")]
    pub funding_gate_allow_timeout: bool,

    /// Per-validator account partitioning (D2): in multi-target mode, each validator's
    /// load draws sender AND receiver only from its own disjoint block of accounts,
    /// so a coin is never reserved on more than one validator. Set false to reproduce
    /// the legacy cross-validator behavior (A/B). No effect in single-target mode.
    /// `ArgAction::Set` so `--partition-accounts true|false` is accepted (a bare bool
    /// field becomes a flag that rejects an explicit value and defaults to false).
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub partition_accounts: bool,

    /// Emit-genesis mode: print a JSON array of N pre-funded test-account entries
    /// (user_001..user_N, address = same blake3 derivation the load uses) and exit.
    /// Splice the output into genesis `accounts` so the accounts are funded at genesis
    /// — then run the benchmark with `--skip-funding` to skip the (slow, conflict-prone)
    /// seed-funding entirely. Uses --init-account-balance and --coins-per-account.
    #[arg(long, default_value = "0")]
    pub emit_genesis_accounts: u64,

    /// Skip the seed-funding phase (accounts are pre-funded at genesis). The funding
    /// gate still runs to confirm the pre-funded accounts are applied on all validators.
    #[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
    pub skip_funding: bool,
}

impl BenchmarkConfig {
    /// Parse validator URLs: if --validator-urls is set, split by comma;
    /// otherwise use the single --validator-url.
    pub fn get_validator_urls(&self) -> Vec<String> {
        if !self.validator_urls.is_empty() {
            self.validator_urls.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            vec![self.validator_url.clone()]
        }
    }

    /// Funding-gate sample count: 0 means "all accounts", otherwise the parsed number.
    pub fn funding_gate_sample_count(&self) -> u64 {
        let v = self.funding_gate_accounts.trim();
        if v.eq_ignore_ascii_case("all") {
            0
        } else {
            v.parse().unwrap_or(8)
        }
    }

    /// Parse subnet IDs from --subnets option.
    pub fn get_subnet_ids(&self) -> Vec<String> {
        if !self.subnets.is_empty() {
            self.subnets.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            vec![]
        }
    }

    /// Parse Move type arguments from comma-separated string.
    pub fn get_move_type_args(&self) -> Vec<String> {
        if !self.move_type_args.is_empty() {
            self.move_type_args.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            vec![]
        }
    }

    /// Parse Move pure arguments from comma-separated string.
    pub fn get_move_args(&self) -> Vec<String> {
        if !self.move_args.is_empty() {
            self.move_args.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            vec![]
        }
    }

    pub fn print_config(&self) {
        info!("Configuration:");
        let urls = self.get_validator_urls();
        if urls.len() > 1 {
            info!("  Validator URLs:   {} validators", urls.len());
            for (i, url) in urls.iter().enumerate() {
                info!("    [{}] {}", i + 1, url);
            }
        } else {
            info!("  Validator URL:    {}", self.validator_url);
        }
        let subnet_ids = self.get_subnet_ids();
        if !subnet_ids.is_empty() {
            info!("  Subnets:          {:?}", subnet_ids);
        }
        info!("  Mode:             {:?}", self.mode);
        info!("  Concurrency:      {}", self.concurrency);
        
        match self.mode {
            BenchmarkMode::Burst => {
                info!("  Total Txns:       {}", self.total);
            }
            BenchmarkMode::Sustained => {
                info!("  Duration:         {}s", self.duration);
                info!("  Target TPS:       {}", self.target_tps);
            }
            BenchmarkMode::Ramp => {
                info!("  Duration:         {}s", self.duration);
                info!("  Ramp Start:       {} TPS", self.ramp_start);
                info!("  Ramp Step:        {} TPS", self.ramp_step);
                info!("  Step Duration:    {}s", self.ramp_step_duration);
            }
        }
        
        info!("  Warmup Txns:      {}", self.warmup);
        info!("  Timeout:          {}s", self.timeout);
        match self.workload {
            WorkloadType::Transfer => {
                if self.init_accounts > 0 {
                    info!("  Init Accounts:    {} (balance: {} each, {} coins/account)", self.init_accounts, self.init_account_balance, self.coins_per_account);
                }
                if self.use_batch {
                    info!("  Batch Mode:       ENABLED");
                    info!("  Batch Size:       {}", self.batch_size);
                } else {
                    info!("  Batch Mode:       disabled (single transfer API)");
                }
            }
            WorkloadType::MoveCall => {
                info!("  Workload:         Move Call");
                info!("  Package:          {}", self.move_package);
                info!("  Module:           {}", self.move_module);
                info!("  Function:         {}", self.move_function);
                if !self.move_type_args.is_empty() {
                    info!("  Type Args:        {}", self.move_type_args);
                }
                if !self.move_args.is_empty() {
                    info!("  Args:             {}", self.move_args);
                }
            }
        }
        info!("");
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;
    use clap::Parser;

    fn parse(extra: &[&str]) -> BenchmarkConfig {
        let mut argv = vec!["setu-benchmark"];
        argv.extend_from_slice(extra);
        BenchmarkConfig::try_parse_from(argv).expect("parse should succeed")
    }

    /// Regression guard: a bare `bool` field with a string `default_value` becomes a
    /// flag that rejects an explicit value and defaults to false. `partition_accounts`
    /// must default TRUE and accept `--partition-accounts true|false` (ArgAction::Set).
    #[test]
    fn partition_accounts_defaults_true_and_takes_value() {
        assert!(parse(&[]).partition_accounts, "default must be true");
        assert!(!parse(&["--partition-accounts", "false"]).partition_accounts);
        assert!(parse(&["--partition-accounts", "true"]).partition_accounts);
    }

    #[test]
    fn funding_gate_sample_count_parsing() {
        assert_eq!(parse(&[]).funding_gate_sample_count(), 8);
        assert_eq!(parse(&["--funding-gate-accounts", "all"]).funding_gate_sample_count(), 0);
        assert_eq!(parse(&["--funding-gate-accounts", "ALL"]).funding_gate_sample_count(), 0);
        assert_eq!(parse(&["--funding-gate-accounts", "20"]).funding_gate_sample_count(), 20);
    }
}
