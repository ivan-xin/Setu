//! Setu Solver - Main entry point

use setu_core::NodeConfig;
use setu_solver::Solver;
use tracing::Level;
use tracing_subscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    // Load configuration from environment
    let config = NodeConfig::from_env();

    // Create and run solver
    let solver = Solver::new(config);
    solver.run().await;

    Ok(())
}

