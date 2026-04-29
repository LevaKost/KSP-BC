//! Detection of the local Kerbal Space Program installation and the layout
//! of its `Ships/VAB` and `Ships/SPH` directories.

mod detector;
mod paths;

pub use detector::{detect_ksp_install, KspInstall};
pub use paths::{candidate_install_roots, BlueprintEntry, ShipType};
