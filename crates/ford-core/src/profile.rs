//! Vehicle profile definitions (YAML).

use crate::error::{Error, Result};
use crate::transport::CanBus;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A known ECU / module on the vehicle network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDef {
    pub id: String,
    pub name: String,
    /// CAN request ID (11-bit), e.g. 0x7E0.
    pub tx_id: u16,
    /// CAN response ID (optional; default tx_id | 0x08).
    #[serde(default)]
    pub rx_id: Option<u16>,
    pub bus: CanBus,
    /// DIDs to try when dumping As-Built / identity.
    #[serde(default)]
    pub dids: Vec<DidDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidDef {
    /// DID as integer (0xF190) or will be parsed from hex string in YAML via custom — use u16.
    pub did: u16,
    pub label: String,
    /// If true, include in As-Built dump.
    #[serde(default)]
    pub as_built: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleProfile {
    pub id: String,
    pub name: String,
    pub years: String,
    pub notes: Option<String>,
    pub modules: Vec<ModuleDef>,
}

impl VehicleProfile {
    pub fn load_yaml(text: &str) -> Result<Self> {
        let profile: Self = serde_yaml::from_str(text)?;
        if profile.modules.is_empty() {
            return Err(Error::Profile("profile has no modules".into()));
        }
        Ok(profile)
    }

    pub fn load_file(path: impl AsRef<Path>) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::load_yaml(&text)
    }

    pub fn module(&self, id: &str) -> Option<&ModuleDef> {
        self.modules.iter().find(|m| m.id == id)
    }
}

/// Built-in profiles shipped with the crate.
pub fn builtin_profiles() -> Result<Vec<VehicleProfile>> {
    let focus = VehicleProfile::load_yaml(include_str!("../../../profiles/focus_2017_eu.yaml"))?;
    let tourneo =
        VehicleProfile::load_yaml(include_str!("../../../profiles/tourneo_connect_2017.yaml"))?;
    Ok(vec![focus, tourneo])
}

pub fn builtin_profile_by_id(id: &str) -> Result<VehicleProfile> {
    builtin_profiles()?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| Error::Profile(format!("unknown profile id: {id}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_focus_profile() {
        let p = VehicleProfile::load_yaml(include_str!("../../../profiles/focus_2017_eu.yaml"))
            .unwrap();
        assert_eq!(p.id, "focus_2017_eu");
        assert!(p.module("PCM").is_some());
        assert!(p.module("BCM").is_some());
        let bcm = p.module("BCM").unwrap();
        assert_eq!(bcm.bus, CanBus::Ms);
    }

    #[test]
    fn load_tourneo_profile() {
        let p =
            VehicleProfile::load_yaml(include_str!("../../../profiles/tourneo_connect_2017.yaml"))
                .unwrap();
        assert_eq!(p.id, "tourneo_connect_2017");
        assert!(!p.modules.is_empty());
    }
}
