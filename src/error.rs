use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Domain-level errors surfaced by the BlueprintShare engine.
#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("serialization error: {0}")]
    Serialize(#[from] bincode::Error),

    #[error("the peer rejected the transfer: {reason}")]
    PeerRejected { reason: String },

    #[error("unexpected protocol message: {0}")]
    Protocol(String),

    #[error("incompatible protocol version: peer reported {peer}, we speak {ours}")]
    VersionMismatch { peer: u16, ours: u16 },

    #[error("checksum mismatch (expected {expected}, got {actual})")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("KSP installation not found")]
    KspNotFound,

    #[error("craft file not found: {0}")]
    CraftNotFound(String),

    #[error("invalid craft file: {0}")]
    InvalidCraft(String),

    #[error("path is not inside the KSP Ships directory: {0}")]
    PathOutsideKsp(PathBuf),

    #[error("transfer too large: {size} bytes exceeds limit of {limit}")]
    TooLarge { size: u64, limit: u64 },
}
