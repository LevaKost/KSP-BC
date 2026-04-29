//! Platform-specific guesses for where KSP keeps its `Ships/` directory.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Whether a blueprint is for a vertical or horizontal launchpad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShipType {
    Vab,
    Sph,
    Unknown,
}

/// A blueprint listed under a KSP install.
#[derive(Debug, Clone)]
pub struct BlueprintEntry {
    pub name: String,
    pub path: PathBuf,
    pub ship_type: ShipType,
    pub size_bytes: u64,
}

/// Default search paths for a KSP install root, ordered most- to
/// least-likely. The detector accepts the first one that exists.
pub fn candidate_install_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(env_root) = std::env::var("KSP_ROOT") {
        if !env_root.is_empty() {
            out.push(PathBuf::from(env_root));
        }
    }

    if let Some(home) = dirs::home_dir() {
        // Steam Proton / Linux native
        out.push(home.join(".local/share/Steam/steamapps/common/Kerbal Space Program"));
        out.push(home.join(".steam/steam/steamapps/common/Kerbal Space Program"));
        out.push(home.join(".local/share/Steam/steamapps/common/Kerbal Space Program 2"));
        // macOS
        out.push(
            home.join("Library/Application Support/Steam/steamapps/common/Kerbal Space Program"),
        );
        out.push(home.join("Applications/Kerbal Space Program"));
        // Catch-all home dirs people use
        out.push(home.join("KSP"));
        out.push(home.join("KSP2"));
    }

    #[cfg(windows)]
    {
        out.extend([
            PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Kerbal Space Program"),
            PathBuf::from(r"C:\Program Files\Steam\steamapps\common\Kerbal Space Program"),
            PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Kerbal Space Program 2"),
        ]);
    }

    out
}
