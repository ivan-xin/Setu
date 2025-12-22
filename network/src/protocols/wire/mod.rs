// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Wire protocol for message framing and serialization

pub mod codec;
pub mod handshake;

pub use codec::{MessageCodec, MessageFrame};
pub use handshake::{Handshake, HandshakeMessage};
