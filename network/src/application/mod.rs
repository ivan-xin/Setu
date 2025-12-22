// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Application-level network interface

pub mod interface;
pub mod service;

pub use interface::NetworkInterface;
pub use service::{NetworkClient, NetworkService};
