//! Light-weight `.craft` parser.
//!
//! KSP1 craft files are plain text with a CFG-style structure, e.g.:
//!
//! ```text
//! ship = Mun Rocket III
//! version = 1.12.5
//! description = ...
//! type = VAB
//! PART
//! { ... }
//! ```
//!
//! KSP2 craft files use a JSON-ish container with `name` / `gameVersion`.
//! We only need a handful of fields, so we do a tolerant scan rather than
//! pulling in a full parser.

use std::path::Path;

use crate::craft::metadata::{CraftMetadata, KspGeneration};
use crate::ksp::ShipType;
use crate::{Error, Result};

/// Parse metadata out of an in-memory craft file. The caller is responsible
/// for filling in `size_bytes` and `sha256` afterwards.
pub fn parse_metadata(bytes: &[u8], path: &Path) -> Result<CraftMetadata> {
    let head = std::str::from_utf8(bytes.get(..bytes.len().min(8 * 1024)).unwrap_or(&[]))
        .map_err(|err| Error::InvalidCraft(format!("not valid UTF-8 near header: {err}")))?;

    let fallback_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("blueprint")
        .to_string();

    if looks_like_ksp2(head) {
        let mut meta =
            CraftMetadata::new(json_field(head, "name").unwrap_or_else(|| fallback_name.clone()));
        meta.ksp_version = json_field(head, "gameVersion");
        meta.ship_type = match json_field(head, "type")
            .or_else(|| json_field(head, "shipType"))
            .as_deref()
        {
            Some(s) if s.eq_ignore_ascii_case("VAB") => ShipType::Vab,
            Some(s) if s.eq_ignore_ascii_case("SPH") => ShipType::Sph,
            _ => ShipType::Unknown,
        };
        meta.generation = KspGeneration::Ksp2;
        return Ok(meta);
    }

    let mut meta =
        CraftMetadata::new(cfg_field(head, "ship").unwrap_or_else(|| fallback_name.clone()));
    meta.ksp_version = cfg_field(head, "version");
    meta.ship_type = match cfg_field(head, "type").as_deref() {
        Some(s) if s.eq_ignore_ascii_case("VAB") => ShipType::Vab,
        Some(s) if s.eq_ignore_ascii_case("SPH") => ShipType::Sph,
        _ => ShipType::Unknown,
    };
    meta.generation = KspGeneration::Ksp1;
    Ok(meta)
}

fn looks_like_ksp2(head: &str) -> bool {
    let trimmed = head.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn cfg_field(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = strip_key(line, key) {
            return Some(rest.to_string());
        }
    }
    None
}

fn strip_key<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let lower = line.to_ascii_lowercase();
    let key_lower = key.to_ascii_lowercase();
    let prefix = format!("{key_lower} =");
    if let Some(rest) = lower.strip_prefix(&prefix) {
        let start = line.len() - rest.len();
        Some(line[start..].trim())
    } else {
        None
    }
}

fn json_field(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let idx = text.find(&needle)?;
    let after = &text[idx + needle.len()..];
    let after = after.trim_start();
    let after = after.strip_prefix(':')?.trim_start();
    if let Some(rest) = after.strip_prefix('"') {
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    } else {
        let end = after.find([',', '}', '\n', '\r']).unwrap_or(after.len());
        Some(after[..end].trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_ksp1_header() {
        let craft = b"ship = Mun Rocket III\nversion = 1.12.5\ntype = VAB\nPART\n{ name=foo }\n";
        let meta = parse_metadata(craft, &PathBuf::from("Mun Rocket III.craft")).unwrap();
        assert_eq!(meta.name, "Mun Rocket III");
        assert_eq!(meta.ksp_version.as_deref(), Some("1.12.5"));
        assert_eq!(meta.ship_type, ShipType::Vab);
        assert_eq!(meta.generation, KspGeneration::Ksp1);
    }

    #[test]
    fn parses_ksp2_header() {
        let craft = br#"{ "name": "Sky Lab", "gameVersion": "0.2.0", "type": "SPH" }"#;
        let meta = parse_metadata(craft, &PathBuf::from("Sky Lab.json")).unwrap();
        assert_eq!(meta.name, "Sky Lab");
        assert_eq!(meta.ksp_version.as_deref(), Some("0.2.0"));
        assert_eq!(meta.ship_type, ShipType::Sph);
        assert_eq!(meta.generation, KspGeneration::Ksp2);
    }

    #[test]
    fn falls_back_to_filename() {
        let craft = b"PART\n{ name=foo }\n";
        let meta = parse_metadata(craft, &PathBuf::from("Lonely Probe.craft")).unwrap();
        assert_eq!(meta.name, "Lonely Probe");
    }
}
