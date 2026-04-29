//! Detect the user's KSP install and walk its blueprint directories.

use std::fs;
use std::path::{Path, PathBuf};

use crate::ksp::paths::{candidate_install_roots, BlueprintEntry, ShipType};
use crate::{Error, Result};

/// A confirmed KSP install on disk.
#[derive(Debug, Clone)]
pub struct KspInstall {
    pub root: PathBuf,
}

impl KspInstall {
    /// `<root>/Ships/VAB`.
    pub fn vab_dir(&self) -> PathBuf {
        self.root.join("Ships").join("VAB")
    }

    /// `<root>/Ships/SPH`.
    pub fn sph_dir(&self) -> PathBuf {
        self.root.join("Ships").join("SPH")
    }

    /// Resolve a blueprint name (with or without a `.craft` suffix) to a path.
    pub fn find_blueprint(&self, name: &str, ship: Option<ShipType>) -> Result<PathBuf> {
        let want = name.trim().trim_end_matches(".craft");
        for entry in self.list_blueprints()? {
            if let Some(filter) = ship {
                if entry.ship_type != filter {
                    continue;
                }
            }
            if entry.name.eq_ignore_ascii_case(want) {
                return Ok(entry.path);
            }
        }
        Err(Error::CraftNotFound(format!(
            "no blueprint named \"{name}\" under {}",
            self.root.display()
        )))
    }

    /// List blueprints stored in `Ships/VAB` and `Ships/SPH`.
    pub fn list_blueprints(&self) -> Result<Vec<BlueprintEntry>> {
        let mut entries = Vec::new();
        collect(&self.vab_dir(), ShipType::Vab, &mut entries)?;
        collect(&self.sph_dir(), ShipType::Sph, &mut entries)?;
        entries.sort_by_key(|entry| entry.name.to_lowercase());
        Ok(entries)
    }
}

/// Probe well-known locations for a KSP install. Returns the first one
/// that exists on disk.
pub fn detect_ksp_install() -> Result<KspInstall> {
    for candidate in candidate_install_roots() {
        if has_ships_dir(&candidate) {
            return Ok(KspInstall { root: candidate });
        }
    }
    Err(Error::KspNotFound)
}

fn has_ships_dir(root: &Path) -> bool {
    root.join("Ships").is_dir()
}

fn collect(dir: &Path, ship: ShipType, out: &mut Vec<BlueprintEntry>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            continue;
        };
        if !ext.eq_ignore_ascii_case("craft") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        out.push(BlueprintEntry {
            name,
            path,
            ship_type: ship,
            size_bytes,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn lists_blueprints_from_synthetic_install() {
        let dir = TempDir::new().unwrap();
        let vab = dir.path().join("Ships").join("VAB");
        let sph = dir.path().join("Ships").join("SPH");
        fs::create_dir_all(&vab).unwrap();
        fs::create_dir_all(&sph).unwrap();
        fs::write(vab.join("Mun Rocket III.craft"), b"ship = Mun Rocket III\n").unwrap();
        fs::write(sph.join("Glider.craft"), b"ship = Glider\n").unwrap();

        let install = KspInstall {
            root: dir.path().to_path_buf(),
        };
        let entries = install.list_blueprints().unwrap();
        assert_eq!(entries.len(), 2);
        let mun = install.find_blueprint("Mun Rocket III", None).unwrap();
        assert!(mun.ends_with("Mun Rocket III.craft"));
    }
}
