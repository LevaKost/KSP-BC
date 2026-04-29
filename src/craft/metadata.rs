//! Strongly-typed metadata extracted from a `.craft` file.

use serde::{Deserialize, Serialize};

use crate::ksp::ShipType;

/// Generation of KSP that produced a blueprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KspGeneration {
    Ksp1,
    Ksp2,
    Unknown,
}

/// Metadata about a blueprint that gets transmitted alongside the file body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftMetadata {
    pub name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub ksp_version: Option<String>,
    pub ship_type: ShipType,
    pub generation: KspGeneration,
}

impl CraftMetadata {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            size_bytes: 0,
            sha256: String::new(),
            ksp_version: None,
            ship_type: ShipType::Unknown,
            generation: KspGeneration::Unknown,
        }
    }
}
