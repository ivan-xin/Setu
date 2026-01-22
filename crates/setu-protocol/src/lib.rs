// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! # Setu Protocol
//!
//! This crate defines the protocol-level message types for Setu network communication.
//! It serves as the bridge between the transport layer (setu-network-anemo) and the
//! application layer (setu-validator).
//!
//! ## Design Goals
//!
//! 1. **Separation of Concerns**: Transport layer should not depend on business types
//! 2. **Protocol Versioning**: Support backward-compatible protocol evolution
//! 3. **Type Safety**: Strong typing for all network messages
//!
//! ## Module Organization
//!
//! - [`message`] - Core `SetuMessage` enum and serialization
//! - [`event`] - `NetworkEvent` for application layer notifications
//! - [`rpc`] - RPC request/response types for state sync
//! - [`version`] - Protocol version management
//!
//! ## Usage
//!
//! ```rust,ignore
//! use setu_protocol::{SetuMessage, NetworkEvent, MessageCodec};
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
pub mod rpc;
pub mod version;

// Re-export main types
pub use message::{SetuMessage, MessageType, MessageCodec};
pub use event::NetworkEvent;
pub use rpc::{
    SerializedEvent, SerializedConsensusFrame, SerializedVote,
    SyncEventsRequest, SyncEventsResponse,
    SyncConsensusFramesRequest, SyncConsensusFramesResponse,
};
pub use version::ProtocolVersion;
