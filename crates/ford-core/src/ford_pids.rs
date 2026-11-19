//! Ford extended PID catalog (Mode 22 DIDs + Mode 01 PIDs with Torque equations).
//!
//! Loads from bundled JSON, resolves headers against VehicleProfile,
//! skips J1850 and Mode 01 PCM duplicates.

use crate::error::{Error, Result};
use crate::profile::{ModuleDef, VehicleProfile};
use crate::transport::CanBus;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// One Torque-style extended PID (Ford predefined set).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FordExtendedPid {
    pub name: String,
    #[serde(rename = "shortName")]
    pub short_name: String,
    #[serde(rename = "modeAndPid")]
    pub mode_and_pid: String,
    pub equation: String,
    #[serde(default)]
    pub min: f64,
    #[serde(default = "default_max")]
    pub max: f64,
    #[serde(default = "default_scale")]
    pub scale: f64,
    #[serde(default)]
    pub unit: String,
    #[serde(default = "default_header")]
    pub header: String,
}

fn default_max() -> f64 {
    100.0
}

fn default_scale() -> f64 {
    1.0
}

fn default_header() -> String {
    "Auto".to_string()
}

impl FordExtendedPid {
    /// Normalized hex string (uppercase, digits/A-F only).
    pub fn mode_and_pid_normalized(&self) -> String {
        self.mode_and_pid
            .trim()
            .to_uppercase()
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .collect()
    }

    /// Extract mode byte.
    pub fn mode(&self) -> Option<u8> {
        let hex = self.mode_and_pid_normalized();
        if hex.len() >= 4 {
            // Long form: 221340 → mode 22
            u8::from_str_radix(&hex[0..2], 16).ok()
        } else if hex.len() == 2 {
            // Short form: 05 → mode 01
            Some(0x01)
        } else {
            None
        }
    }

    /// Mode 22 DID or Mode 01 PID id.
    pub fn pid_or_did(&self) -> Option<u16> {
        let hex = self.mode_and_pid_normalized();
        if hex.len() >= 4 {
            // Long form: 221340 → DID 0x1340
            u16::from_str_radix(&hex[2..], 16).ok()
        } else if hex.len() == 2 {
            // Short form: 05 → PID 0x05
            u8::from_str_radix(&hex, 16).ok().map(u16::from)
        } else {
            None
        }
    }

    pub fn is_mode22(&self) -> bool {
        self.mode() == Some(0x22)
    }

    pub fn is_mode01(&self) -> bool {
        self.mode() == Some(0x01)
    }
}

/// Ford extended PID bound to a concrete profile module (or synthetic header module).
#[derive(Debug, Clone)]
pub struct ResolvedFordPid {
    pub pid: FordExtendedPid,
    pub module_id: String,
    pub module: ModuleDef,
}

/// Load the bundled Ford extended PID catalog from JSON.
pub fn load_catalog() -> Result<Vec<FordExtendedPid>> {
    let json_str = include_str!("../assets/ford/extended_pids.json");
    serde_json::from_str(json_str).map_err(Error::Json)
}

/// Load the Mode 01 PIDs that responded in a reference TorqueScan export.
/// Used to mark/prioritize OBD catalog rows.
pub fn load_mode01_scan_supported() -> Result<HashSet<u8>> {
    let json_str = include_str!("../assets/ford/mode01_scan_supported.json");
    let list: Vec<u8> = serde_json::from_str(json_str).map_err(Error::Json)?;
    Ok(list.into_iter().collect())
}

/// Map Torque header alias to a profile module.
/// Returns None when the header is unsupported (J1850) or a required alias module is missing.
pub fn resolve_ford_header(header: &str, profile: &VehicleProfile) -> Option<ModuleDef> {
    let h = header.trim();
    
    // Skip J1850 PWM
    if h.eq_ignore_ascii_case("C410F1") {
        return None;
    }

    let up = h.to_uppercase();
    match up.as_str() {
        "" | "AUTO" => return profile.module("PCM").cloned(),
        "TCM" => return profile.module("TCM").cloned(),
        "ABS" => return profile.module("ABS").cloned(),
        "INS" | "IPC" => return profile.module("IPC").cloned(),
        "PAM" => return profile.module("PAM").cloned(),
        _ => {}
    }

    // Try parsing as hex TX ID
    if let Ok(tx) = u16::from_str_radix(&h, 16) {
        // Look for existing module with this TX ID
        if let Some(module) = profile.modules.iter().find(|m| m.tx_id == tx) {
            return Some(module.clone());
        }
        // Synthetic module for known hex headers (e.g. TPMS 0x726)
        let id = format!("HDR_{:03X}", tx);
        return Some(ModuleDef {
            id,
            name: format!("Header 0x{:03X}", tx),
            tx_id: tx,
            rx_id: None,
            bus: CanBus::Hs,
            security_level: None,
            security_algo: None,
            dids: vec![],
            params: vec![],
        });
    }

    None
}

/// Resolve catalog entries that can be polled with the given profile.
/// Skips:
/// - Unsupported modes/headers
/// - Mode 01 on PCM (duplicates SAE Mode 01)
/// - Mode 01 on synthetic HDR_* modules (only real alias modules)
pub fn resolve_ford_extended_pids(
    catalog: &[FordExtendedPid],
    profile: &VehicleProfile,
) -> Vec<ResolvedFordPid> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for pid in catalog {
        let mode = match pid.mode() {
            Some(0x01) | Some(0x22) => pid.mode().unwrap(),
            _ => continue,
        };
        let pid_or_did = match pid.pid_or_did() {
            Some(v) if v > 0 => v,
            _ => continue,
        };

        let module = match resolve_ford_header(&pid.header, profile) {
            Some(m) => m,
            None => continue,
        };

        // Mode 01 on PCM duplicates SAE Mode 01 — skip
        if pid.is_mode01() && module.id == "PCM" {
            continue;
        }

        // Mode 01 only when alias module exists (e.g. TCM); skip synthetic HDR_*
        if pid.is_mode01() && module.id.starts_with("HDR_") {
            continue;
        }

        // Mode 01 only when the module is in the profile (not just synthetic)
        if pid.is_mode01() && profile.module(&module.id).is_none() {
            continue;
        }

        let key = format!(
            "{}|{}|{}|{}",
            module.id, mode, pid_or_did, pid.equation
        );
        if !seen.insert(key) {
            continue;
        }

        out.push(ResolvedFordPid {
            pid: pid.clone(),
            module_id: module.id.clone(),
            module,
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_catalog() {
        let catalog = load_catalog().unwrap();
        assert!(!catalog.is_empty());
        // Check first entry is Coolant from TCM
        assert!(catalog[0].name.contains("Coolant"));
        assert_eq!(catalog[0].header, "TCM");
    }

    #[test]
    fn test_load_mode01_scan() {
        let supported = load_mode01_scan_supported().unwrap();
        assert!(!supported.is_empty());
        assert!(supported.contains(&0x05)); // Coolant temp
        assert!(supported.contains(&0x0C)); // RPM
    }

    #[test]
    fn test_mode_and_pid_parsing() {
        let pid = FordExtendedPid {
            name: "Test".into(),
            short_name: "T".into(),
            mode_and_pid: "221340".into(),
            equation: "A".into(),
            min: 0.0,
            max: 100.0,
            scale: 1.0,
            unit: "".into(),
            header: "Auto".into(),
        };
        assert_eq!(pid.mode(), Some(0x22));
        assert_eq!(pid.pid_or_did(), Some(0x1340));
        assert!(pid.is_mode22());
        assert!(!pid.is_mode01());
    }

    #[test]
    fn test_mode01_short_form() {
        let pid = FordExtendedPid {
            name: "Test".into(),
            short_name: "T".into(),
            mode_and_pid: "05".into(),
            equation: "A-40".into(),
            min: 0.0,
            max: 100.0,
            scale: 1.0,
            unit: "°C".into(),
            header: "TCM".into(),
        };
        assert_eq!(pid.mode(), Some(0x01));
        assert_eq!(pid.pid_or_did(), Some(0x05));
        assert!(pid.is_mode01());
    }

    #[test]
    fn test_resolve_header_j1850_skip() {
        let profile = VehicleProfile {
            id: "test".into(),
            name: "Test".into(),
            years: "2020".into(),
            notes: None,
            modules: vec![],
        };
        // J1850 should be skipped
        assert!(resolve_ford_header("C410F1", &profile).is_none());
        
        // Hex header should create synthetic module
        let module = resolve_ford_header("726", &profile).unwrap();
        assert_eq!(module.id, "HDR_726");
        assert_eq!(module.tx_id, 0x726);
    }
}
