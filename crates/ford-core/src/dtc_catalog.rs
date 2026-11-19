//! OBD-II generic DTC catalog lookup.

use crate::uds::Dtc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// DTC name and description from the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DtcInfo {
    pub name: String,
    pub description: String,
}

static CATALOG: OnceLock<HashMap<String, DtcInfo>> = OnceLock::new();

/// Load the built-in OBD-II generic DTC catalog.
fn load_catalog() -> HashMap<String, DtcInfo> {
    const JSON: &str = include_str!("../assets/dtc/obd_generic.json");
    serde_json::from_str(JSON).unwrap_or_default()
}

/// Lookup DTC information by code (e.g., "P0301" or "P030100").
/// Normalizes by stripping trailing zeros to match catalog format.
pub fn lookup_dtc_info(code: &str) -> Option<DtcInfo> {
    let catalog = CATALOG.get_or_init(load_catalog);
    let normalized = normalize_dtc_code(code);
    catalog.get(&normalized).cloned()
}

/// Normalize DTC code by stripping trailing zeros and converting to uppercase.
/// "P030100" -> "P0301", "p0301" -> "P0301"
fn normalize_dtc_code(code: &str) -> String {
    let upper = code.to_uppercase();
    // Strip trailing zeros after the first 5 characters (PXXXX format)
    if upper.len() > 5 {
        upper.trim_end_matches('0').to_string()
    } else {
        upper
    }
}

/// Format a DTC with catalog info for display.
pub fn format_dtc_display(dtc: &Dtc) -> String {
    let info = lookup_dtc_info(&dtc.code);
    let status = format_dtc_status(dtc.status);
    
    match info {
        Some(ref i) => {
            format!("{} ({}): {} — {}", i.name, dtc.code, status, i.description)
        }
        None => {
            format!("Unknown ({}): {}", dtc.code, status)
        }
    }
}

/// Format DTC status byte for display.
pub fn format_dtc_status(status: u8) -> String {
    let mut parts = Vec::new();
    if status & 0x01 != 0 {
        parts.push("TestFailed");
    }
    if status & 0x04 != 0 {
        parts.push("Pending");
    }
    if status & 0x08 != 0 {
        parts.push("Confirmed");
    }
    if status & 0x10 != 0 {
        parts.push("TestNotCompletedSinceLastClear");
    }
    if status & 0x20 != 0 {
        parts.push("TestFailedSinceLastClear");
    }
    if status & 0x40 != 0 {
        parts.push("TestNotCompletedThisOpCycle");
    }
    if status & 0x80 != 0 {
        parts.push("WarningIndicatorRequested");
    }
    if parts.is_empty() {
        "NotActive".to_string()
    } else {
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_p0301() {
        let info = lookup_dtc_info("P0301").unwrap();
        assert_eq!(info.name, "Cylinder 1 Misfire");
        assert!(info.description.contains("Cylinder 1 Misfire"));
    }

    #[test]
    fn lookup_lowercase() {
        let info = lookup_dtc_info("p0301").unwrap();
        assert_eq!(info.name, "Cylinder 1 Misfire");
    }

    #[test]
    fn lookup_with_trailing_zeros() {
        // format_dtc_iso produces 6-char codes like P030100, catalog uses 5-char P0301
        let info = lookup_dtc_info("P030100").unwrap();
        assert_eq!(info.name, "Cylinder 1 Misfire");
    }

    #[test]
    fn lookup_missing() {
        let info = lookup_dtc_info("P9999");
        assert!(info.is_none());
    }

    #[test]
    fn format_status_confirmed() {
        let status = format_dtc_status(0x09); // TestFailed + Confirmed
        assert!(status.contains("TestFailed"));
        assert!(status.contains("Confirmed"));
    }

    #[test]
    fn format_dtc_with_catalog() {
        let dtc = Dtc::from_bytes(0x03, 0x01, 0x00, 0x09);
        let display = format_dtc_display(&dtc);
        eprintln!("DTC code: {}", dtc.code);
        eprintln!("Display: {}", display);
        assert!(display.contains("Cylinder 1 Misfire"));
        assert!(display.contains("P0301"));
    }
}
