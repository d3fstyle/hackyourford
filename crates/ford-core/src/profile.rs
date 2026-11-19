//! Vehicle profile definitions (YAML and JSON).

use crate::error::{Error, Result};
use crate::transport::CanBus;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    /// Odd SecurityAccess requestSeed level (e.g. 1). When set, unlock before As-Built write.
    #[serde(default)]
    pub security_level: Option<u8>,
    /// Key algorithm id (`experimental_xor_ff` for demo). Ignored if `security_level` is unset.
    #[serde(default)]
    pub security_algo: Option<String>,
    /// DIDs to try when dumping As-Built / identity.
    #[serde(default)]
    pub dids: Vec<DidDef>,
    /// Named As-Built bit-field parameters (decode UI). Empty = hex-only.
    #[serde(default)]
    pub params: Vec<ParamDef>,
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

/// How a parameter value is interpreted in the UI.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParamKind {
    Bool,
    Uint,
    Enum,
}

/// One selectable value for an enum parameter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParamOption {
    pub value: u64,
    pub label: String,
}

/// Named bit-field inside an As-Built DID block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDef {
    pub id: String,
    pub label: String,
    pub did: u16,
    /// Byte offset within the DID payload (0-based).
    pub byte: u16,
    /// Bit offset within that byte (0–7), LSB = 0.
    pub bit: u8,
    /// Field width in bits (1 for bool).
    #[serde(default = "default_param_width")]
    pub width: u8,
    pub kind: ParamKind,
    #[serde(default)]
    pub options: Vec<ParamOption>,
    /// When false, UI warns before writing this field.
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub notes: Option<String>,
}

fn default_param_width() -> u8 {
    1
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

    pub fn load_json(text: &str) -> Result<Self> {
        let profile: Self = serde_json::from_str(text)?;
        if profile.modules.is_empty() {
            return Err(Error::Profile("profile has no modules".into()));
        }
        Ok(profile)
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| Error::Profile(e.to_string()))
    }

    pub fn load_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            Self::load_json(&text)
        } else {
            Self::load_yaml(&text)
        }
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

fn builtin_ids() -> Vec<String> {
    vec!["focus_2017_eu".to_string(), "tourneo_connect_2017".to_string()]
}

/// User profile store that manages profiles in a directory.
pub struct UserProfileStore {
    user_dir: PathBuf,
}

impl UserProfileStore {
    /// Create a new profile store. The directory will be created if it doesn't exist.
    pub fn new(user_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&user_dir)?;
        Ok(Self { user_dir })
    }

    fn user_file(&self, id: &str) -> PathBuf {
        self.user_dir.join(format!("{}.json", id))
    }

    /// Load all user profiles from disk.
    pub fn load_user_profiles(&self) -> Result<Vec<VehicleProfile>> {
        let mut profiles = Vec::new();
        
        if !self.user_dir.exists() {
            return Ok(profiles);
        }

        for entry in std::fs::read_dir(&self.user_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(profile) = VehicleProfile::load_file(&path) {
                    profiles.push(profile);
                }
            }
        }
        
        profiles.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(profiles)
    }

    /// Check if a profile exists on disk.
    pub fn is_on_disk(&self, id: &str) -> bool {
        self.user_file(id).exists()
    }

    /// Check if a profile can be deleted (exists on disk).
    pub fn can_delete(&self, id: &str) -> bool {
        self.is_on_disk(id)
    }

    /// Save a profile to disk as JSON.
    pub fn save(&self, profile: &VehicleProfile) -> Result<()> {
        if profile.id.trim().is_empty() {
            return Err(Error::Profile("profile id required".into()));
        }
        if profile.modules.is_empty() {
            return Err(Error::Profile("profile must have modules".into()));
        }
        
        let safe_id = profile.id.trim();
        if !safe_id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Err(Error::Profile(
                "profile id must be alphanumeric/underscore/dash".into(),
            ));
        }

        let json = profile.to_json()?;
        std::fs::write(self.user_file(safe_id), json)?;
        Ok(())
    }

    /// Delete a profile from disk.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let file = self.user_file(id);
        if !file.exists() {
            return Ok(false);
        }
        std::fs::remove_file(file)?;
        Ok(true)
    }
}

/// Profile metadata for listing (nested profile for a stable frontend shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileListItem {
    pub profile: VehicleProfile,
    pub is_builtin: bool,
    pub can_delete: bool,
}

/// List all profiles: built-ins + user profiles (user profiles shadow built-ins with same id).
pub fn list_all_profiles(user_dir: Option<PathBuf>) -> Result<Vec<ProfileListItem>> {
    let builtin_ids_set: std::collections::HashSet<String> = builtin_ids().into_iter().collect();
    
    // Load built-ins
    let mut by_id: std::collections::HashMap<String, ProfileListItem> = builtin_profiles()?
        .into_iter()
        .map(|p| {
            let id = p.id.clone();
            (
                id,
                ProfileListItem {
                    profile: p,
                    is_builtin: true,
                    can_delete: false,
                },
            )
        })
        .collect();

    // Load and overlay user profiles
    if let Some(dir) = user_dir {
        let store = UserProfileStore::new(dir)?;
        for p in store.load_user_profiles()? {
            let id = p.id.clone();
            let is_builtin = builtin_ids_set.contains(&id);
            by_id.insert(
                id,
                ProfileListItem {
                    profile: p,
                    is_builtin,
                    can_delete: true,
                },
            );
        }
    }

    // Sort by id
    let mut profiles: Vec<ProfileListItem> = by_id.into_values().collect();
    profiles.sort_by(|a, b| a.profile.id.cmp(&b.profile.id));
    Ok(profiles)
}

/// Create a new blank profile template (seeded with a PCM module so it can be saved).
pub fn new_blank_profile(id: String, name: String, years: String) -> VehicleProfile {
    VehicleProfile {
        id,
        name,
        years,
        notes: None,
        modules: vec![ModuleDef {
            id: "PCM".into(),
            name: "Powertrain Control Module".into(),
            tx_id: 0x7E0,
            rx_id: None,
            bus: CanBus::Hs,
            security_level: None,
            security_algo: None,
            dids: vec![DidDef {
                did: 0xF190,
                label: "VIN".into(),
                as_built: false,
            }],
            params: vec![],
        }],
    }
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
        assert!(!bcm.params.is_empty());
        assert!(bcm.params.iter().any(|p| p.id == "drl_enable"));
    }

    #[test]
    fn load_tourneo_profile() {
        let p =
            VehicleProfile::load_yaml(include_str!("../../../profiles/tourneo_connect_2017.yaml"))
                .unwrap();
        assert_eq!(p.id, "tourneo_connect_2017");
        assert!(!p.modules.is_empty());
        let bcm = p.module("BCM").unwrap();
        assert!(!bcm.params.is_empty());
    }

    #[test]
    fn json_serialization_roundtrip() {
        let original = VehicleProfile {
            id: "test_profile".to_string(),
            name: "Test Profile".to_string(),
            years: "2020-2023".to_string(),
            notes: Some("Test notes".to_string()),
            modules: vec![ModuleDef {
                id: "TEST".to_string(),
                name: "Test Module".to_string(),
                tx_id: 0x7E0,
                rx_id: Some(0x7E8),
                bus: CanBus::Hs,
                security_level: Some(1),
                security_algo: Some("test_algo".to_string()),
                dids: vec![DidDef {
                    did: 0xF190,
                    label: "VIN".to_string(),
                    as_built: false,
                }],
                params: vec![],
            }],
        };

        let json = original.to_json().unwrap();
        let parsed = VehicleProfile::load_json(&json).unwrap();

        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.name, original.name);
        assert_eq!(parsed.modules.len(), 1);
        assert_eq!(parsed.modules[0].id, "TEST");
    }

    #[test]
    fn user_profile_store_operations() {
        let temp_dir = std::env::temp_dir().join(format!("ford_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        
        let store = UserProfileStore::new(temp_dir.clone()).unwrap();

        let profile = VehicleProfile {
            id: "test_user_profile".to_string(),
            name: "Test User Profile".to_string(),
            years: "2023".to_string(),
            notes: None,
            modules: vec![ModuleDef {
                id: "TEST".to_string(),
                name: "Test Module".to_string(),
                tx_id: 0x7E0,
                rx_id: None,
                bus: CanBus::Hs,
                security_level: None,
                security_algo: None,
                dids: vec![],
                params: vec![],
            }],
        };

        // Save
        store.save(&profile).unwrap();
        assert!(store.is_on_disk("test_user_profile"));
        assert!(store.can_delete("test_user_profile"));

        // Load
        let loaded = store.load_user_profiles().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "test_user_profile");

        // Delete
        assert!(store.delete("test_user_profile").unwrap());
        assert!(!store.is_on_disk("test_user_profile"));
        assert!(!store.delete("test_user_profile").unwrap());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn list_all_profiles_merges_builtin_and_user() {
        let temp_dir = std::env::temp_dir().join(format!("ford_test_list_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        
        let store = UserProfileStore::new(temp_dir.clone()).unwrap();

        // Create a user profile with same id as builtin
        let override_profile = VehicleProfile {
            id: "focus_2017_eu".to_string(),
            name: "Focus 2017 EU Custom".to_string(),
            years: "2017".to_string(),
            notes: Some("User override".to_string()),
            modules: vec![ModuleDef {
                id: "TEST".to_string(),
                name: "Test Module".to_string(),
                tx_id: 0x7E0,
                rx_id: None,
                bus: CanBus::Hs,
                security_level: None,
                security_algo: None,
                dids: vec![],
                params: vec![],
            }],
        };

        // Create a unique user profile
        let user_profile = VehicleProfile {
            id: "custom_profile".to_string(),
            name: "Custom Profile".to_string(),
            years: "2023".to_string(),
            notes: None,
            modules: vec![ModuleDef {
                id: "CUSTOM".to_string(),
                name: "Custom Module".to_string(),
                tx_id: 0x700,
                rx_id: None,
                bus: CanBus::Ms,
                security_level: None,
                security_algo: None,
                dids: vec![],
                params: vec![],
            }],
        };

        store.save(&override_profile).unwrap();
        store.save(&user_profile).unwrap();

        let all = list_all_profiles(Some(temp_dir.clone())).unwrap();
        
        // Should have custom_profile + focus_2017_eu (overridden) + tourneo_connect_2017
        assert_eq!(all.len(), 3);

        let focus = all.iter().find(|p| p.profile.id == "focus_2017_eu").unwrap();
        assert_eq!(focus.profile.name, "Focus 2017 EU Custom");
        assert!(focus.is_builtin);
        assert!(focus.can_delete);

        let custom = all.iter().find(|p| p.profile.id == "custom_profile").unwrap();
        assert!(!custom.is_builtin);
        assert!(custom.can_delete);

        let tourneo = all.iter().find(|p| p.profile.id == "tourneo_connect_2017").unwrap();
        assert!(tourneo.is_builtin);
        assert!(!tourneo.can_delete);

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
