//! Targeted tests for the metadata parser.

use std::path::PathBuf;

use ksp_blueprintshare::craft::{parse_metadata, KspGeneration};
use ksp_blueprintshare::ksp::ShipType;

#[test]
fn parses_ksp1_craft_with_quoted_values() {
    let craft = b"ship = Lonely Probe\nversion = 1.12.5\ntype = SPH\n";
    let meta = parse_metadata(craft, &PathBuf::from("Lonely Probe.craft")).unwrap();
    assert_eq!(meta.name, "Lonely Probe");
    assert_eq!(meta.ship_type, ShipType::Sph);
    assert_eq!(meta.generation, KspGeneration::Ksp1);
}

#[test]
fn parses_ksp2_blueprint_json() {
    let craft = br#"{"name": "Sky Lab", "gameVersion": "0.2.0", "type": "VAB"}"#;
    let meta = parse_metadata(craft, &PathBuf::from("Sky Lab.json")).unwrap();
    assert_eq!(meta.name, "Sky Lab");
    assert_eq!(meta.ship_type, ShipType::Vab);
    assert_eq!(meta.generation, KspGeneration::Ksp2);
}
