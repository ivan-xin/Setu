//! Router command handlers

use crate::config::Config;
use colored::Colorize;
use anyhow::Result;

pub async fn handle(action: crate::RouterAction, _config: &Config) -> Result<()> {
    match action {
        crate::RouterAction::Status { address } => {
            println!("{} Querying router status...", "→".cyan().bold());
            println!("  Router: {}", address.cyan());
            
            // TODO: Implement actual RPC call
            println!("{} Status query not yet implemented", 
                "!".yellow().bold()
            );
            
            Ok(())
        }
        
        crate::RouterAction::Solvers { address } => {
            println!("{} Listing registered solvers...", "→".cyan().bold());
            println!("  Router: {}", address.cyan());
            
            // TODO: Implement actual RPC call
            println!("{} Solvers list not yet implemented", 
                "!".yellow().bold()
            );
            
            Ok(())
        }
    }
}

