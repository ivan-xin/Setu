// Copyright (c) Hetu Project
// SPDX-License-Identifier: Apache-2.0

//! Error types for Setu key management.

use thiserror::Error;

/// Errors that can occur during key operations.
#[derive(Error, Debug)]
pub enum KeyError {
    #[error("Key generation failed: {0}")]
    KeyGeneration(String),

    #[error("Invalid derivation path: {0}")]
    InvalidDerivationPath(String),

    #[error("Unsupported signature scheme: {0}")]
    UnsupportedScheme(String),

    #[error("Invalid key format: {0}")]
    InvalidKeyFormat(String),

    #[error("Invalid mnemonic phrase: {0}")]
    InvalidMnemonic(String),

    #[error("Signature verification failed: {0}")]
    SignatureVerification(String),

    #[error("Key not found for address: {0}")]
    KeyNotFound(String),

    #[error("Alias already exists: {0}")]
    AliasExists(String),

    #[error("Alias not found: {0}")]
    AliasNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Decoding error: {0}")]
    Decoding(String),
}

impl From<KeyError> for signature::Error {
    fn from(e: KeyError) -> Self {
        signature::Error::from_source(e)
    }
}
