use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("RocksDB error: {0}")]
    RocksDB(#[from] rocksdb::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Column family not found: {0}")]
    ColumnFamilyNotFound(String),

    #[error("Key not found: {0}")]
    KeyNotFound(String),

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Other error: {0}")]
    Other(String),
}

impl From<bcs::Error> for StorageError {
    fn from(e: bcs::Error) -> Self {
        StorageError::Deserialization(e.to_string())
    }
}

// bincode 2.0 uses DecodeError and EncodeError
impl From<bincode::error::DecodeError> for StorageError {
    fn from(e: bincode::error::DecodeError) -> Self {
        StorageError::Deserialization(e.to_string())
    }
}

impl From<bincode::error::EncodeError> for StorageError {
    fn from(e: bincode::error::EncodeError) -> Self {
        StorageError::Serialization(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, StorageError>;
