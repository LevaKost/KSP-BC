//! `.craft` blueprint loading and metadata extraction.

mod metadata;
mod parser;

pub use metadata::{CraftMetadata, KspGeneration};
pub use parser::parse_metadata;

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::ksp::ShipType;
use crate::{Error, Result};

/// A loaded blueprint plus its on-disk bytes.
#[derive(Debug, Clone)]
pub struct CraftFile {
    pub path: PathBuf,
    pub metadata: CraftMetadata,
    pub bytes: Vec<u8>,
}

impl CraftFile {
    /// Load a craft file from disk and parse its metadata.
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)
            .map_err(|err| Error::CraftNotFound(format!("{}: {err}", path.display())))?;
        let mut metadata = parse_metadata(&bytes, path)?;
        metadata.size_bytes = bytes.len() as u64;
        metadata.sha256 = sha256_hex(&bytes);
        if matches!(metadata.ship_type, ShipType::Unknown) {
            metadata.ship_type = ship_type_from_path(path);
        }
        Ok(Self {
            path: path.to_path_buf(),
            metadata,
            bytes,
        })
    }
}

/// Compute the lowercase hex SHA-256 of a buffer.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn ship_type_from_path(path: &Path) -> ShipType {
    for component in path.components() {
        let s = component.as_os_str().to_string_lossy();
        if s.eq_ignore_ascii_case("VAB") {
            return ShipType::Vab;
        }
        if s.eq_ignore_ascii_case("SPH") {
            return ShipType::Sph;
        }
    }
    ShipType::Unknown
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        const TABLE: &[u8; 16] = b"0123456789abcdef";
        let bytes = bytes.as_ref();
        let mut out = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            out.push(TABLE[(b >> 4) as usize] as char);
            out.push(TABLE[(b & 0x0f) as usize] as char);
        }
        out
    }
}

pub use hex::encode as hex_encode;
