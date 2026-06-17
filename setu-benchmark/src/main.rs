//! Setu TPS Benchmark Tool
//!
//! A comprehensive benchmark tool to measure the TPS (Transactions Per Second)
//! of the Setu network.
//!
//! # Usage
//!
//! ```bash
//! # Run with default settings (100 transactions, 10 concurrent)
//! setu-benchmark --validator-url http://127.0.0.1:8080
//!
//! # Run with custom parameters
//! setu-benchmark --validator-url http://127.0.0.1:8080 \
//!     --total 1000 \
//!     --concurrency 50 \
//!     --duration 60
//!
//! # Run sustained load test
//! setu-benchmark --validator-url http://127.0.0.1:8080 \
//!     --mode sustained \
//!     --duration 300 \
//!     --target-tps 1000
//! ```

mod benchmark;
mod client;
mod config;
mod metrics;
mod report;

use clap::Parser;
use config::BenchmarkConfig;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    // Parse command line arguments
    let config = BenchmarkConfig::parse();

    // Emit-genesis mode: print pre-funded account entries and exit (no benchmark run).
    if config.emit_genesis_accounts > 0 {
        println!(
            "{}",
            client::emit_genesis_accounts_json(
                config.emit_genesis_accounts,
                config.init_account_balance,
                config.coins_per_account,
            )
        );
        return Ok(());
    }

    info!("╔══════════════════════════════════════════════════════════╗");
    info!("║            Setu TPS Benchmark Tool v0.1.0                ║");
    info!("╚══════════════════════════════════════════════════════════╝");
    info!("");

    // Print configuration
    config.print_config();

    // Save output format + M0 settings before config is moved
    let output_format = config.output.clone();
    let m0_enabled = config.m0;
    let m0_url = config.validator_url.clone();

    // M0: reset the validator's measurement window before applying load.
    if m0_enabled {
        match report::m0_reset(&m0_url).await {
            Ok(()) => info!("M0: reset measurement window on {}", m0_url),
            Err(e) => info!("M0: reset failed ({e}); is the validator built with --features m0-profiling?"),
        }
    }

    // Run benchmark
    let runner = benchmark::BenchmarkRunner::new(config);
    let result = runner.run().await?;

    // Print report (supports text and json formats)
    match output_format.as_str() {
        "json" => println!("{}", report::json_report(&result)),
        _ => report::print_report(&result),
    }

    // M0: fetch and render the per-stage timing breakdown.
    if m0_enabled {
        match report::m0_fetch(&m0_url).await {
            Ok(jsonl) => report::print_m0_report(&jsonl),
            Err(e) => info!("M0: fetch failed ({e}); is the validator built with --features m0-profiling?"),
        }
    }

    Ok(())
}
