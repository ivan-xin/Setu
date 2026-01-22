// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Protocol module for Setu consensus messages
//!
//! This module contains all consensus-specific message types, network events,
//! and sync protocol definitions. By placing these in the validator crate,
//! we achieve:
//!
//! 1. **Decoupled network layer**: `setu-network` remains generic and consensus-agnostic
//! 2. **Consensus replaceability**: Different consensus implementations can define
//!    their own message types without modifying the network layer
//! 3. **Clear ownership**: Message types belong to the consensus that uses them
//!
//! ## Module Structure
//!
//! - [`message`] - `SetuMessage` enum for network communication
//! - [`event`] - `NetworkEvent` for application layer notifications
//! - [`sync`] - RPC types for state synchronization
//! - [`codec`] - Message encoding/decoding utilities
//!
//! ## Usage
//!
//! ```rust,ignore
//! use setu_validator::protocol::{SetuMessage, NetworkEvent, MessageCodec};
//!
//! // Serialize a message
//! let msg = SetuMessage::Ping { timestamp: 123, nonce: 456 };
//! let bytes = MessageCodec::encode(&msg)?;
//!
//! // Deserialize a message
//! let decoded: SetuMessage = MessageCodec::decode(&bytes)?;
//! ```

pub mod message;
pub mod event;
pub mod sync;
pub mod codec;

// Re-export main types for convenience
pub use message::{SetuMessage, MessageType};
pub use event::NetworkEvent;
pub use sync::{
    SerializedEvent, SerializedConsensusFrame, SerializedVote,
    SyncEventsRequest, SyncEventsResponse,
    SyncConsensusFramesRequest, SyncConsensusFramesResponse,
    PushEventsRequest, PushEventsResponse,
    PushConsensusFrameRequest, PushConsensusFrameResponse,
    PeerSyncInfo, GetSyncStateRequest, GetSyncStateResponse,
};
pub use codec::{MessageCodec, MessageCodecError, Encodable, Decodable};
