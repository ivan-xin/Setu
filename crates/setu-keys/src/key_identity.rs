// Copyright (c) Hetu Project
// SPDX-License-Identifier: Apache-2.0

//! Key identity types for addressing keys by address or alias.

use crate::crypto::SetuAddress;
use crate::error::KeyError;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};
use std::str::FromStr;

/// An address or an alias associated with a key in the wallet.
///
/// This enables users to use either an address or a friendly alias
/// to reference keys in the keystore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KeyIdentity {
    Address(SetuAddress),
    Alias(String),
}

impl FromStr for KeyIdentity {
    type Err = KeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("0x") {
            Ok(KeyIdentity::Address(SetuAddress::from_str(s)?))
        } else {
            Ok(KeyIdentity::Alias(s.to_string()))
        }
    }
}

impl From<SetuAddress> for KeyIdentity {
    fn from(addr: SetuAddress) -> Self {
        KeyIdentity::Address(addr)
    }
}

impl Display for KeyIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyIdentity::Address(addr) => write!(f, "{}", addr),
            KeyIdentity::Alias(alias) => write!(f, "{}", alias),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_address() {
        let addr_str = "0x0000000000000000000000000000000000000000000000000000000000000001";
        let identity: KeyIdentity = addr_str.parse().unwrap();
        assert!(matches!(identity, KeyIdentity::Address(_)));
    }

    #[test]
    fn test_parse_alias() {
        let alias = "my-wallet";
        let identity: KeyIdentity = alias.parse().unwrap();
        assert!(matches!(identity, KeyIdentity::Alias(_)));
        if let KeyIdentity::Alias(a) = identity {
            assert_eq!(a, "my-wallet");
        }
    }
}
