//! Setu RPC - Network communication layer
//!
//! This module provides RPC interfaces for communication between:
//! - Router -> Solver (transfer dispatch)
//! - Solver -> Validator (event submission)
//! - Solver -> Validator (registration)
//! - CLI -> Validator (registration commands)
//!
//! Uses Anemo for high-performance P2P RPC communication.

pub mod router;
pub mod solver;
pub mod validator;
pub mod registration;
pub mod error;
pub mod messages;

pub use error::{RpcError, Result};
pub use messages::*;
pub use registration::*;

