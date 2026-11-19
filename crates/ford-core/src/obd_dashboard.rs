//! OBD Dashboard persistence (from Android ObdDashboard.kt).

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Scope for dashboard storage: global or profile-specific.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DashboardScope {
    Global,
    Profile,
}

/// Key identifying a signal within a dashboard.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObdDashboardSignalKey {
    pub source: String, // "pid", "did", "ford_extended"
    pub id: String,     // "0C" (hex), "PCM:1340" (module:did)
}

/// A saved dashboard configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObdDashboard {
    pub id: String,
    pub name: String,
    pub scope: DashboardScope,
    pub profile_id: Option<String>,
    pub signals: Vec<ObdDashboardSignalKey>,
    #[serde(default)]
    pub created_at_unix_ms: u128,
    #[serde(default)]
    pub updated_at_unix_ms: u128,
}

impl ObdDashboard {
    pub fn new(id: String, name: String, scope: DashboardScope, profile_id: Option<String>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        
        Self {
            id,
            name,
            scope,
            profile_id,
            signals: Vec::new(),
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
        }
    }
}

/// Store for OBD dashboards with global and per-profile directories.
pub struct ObdDashboardStore {
    root: PathBuf,
}

impl ObdDashboardStore {
    pub fn new(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root)?;
        std::fs::create_dir_all(root.join("global"))?;
        Ok(Self { root })
    }

    fn global_dir(&self) -> PathBuf {
        self.root.join("global")
    }

    fn profile_dir(&self, profile_id: &str) -> PathBuf {
        self.root.join("by_profile").join(profile_id)
    }

    fn dashboard_file(&self, scope: DashboardScope, profile_id: Option<&str>, id: &str) -> PathBuf {
        let dir = match scope {
            DashboardScope::Global => self.global_dir(),
            DashboardScope::Profile => {
                let pid = profile_id.unwrap_or("default");
                self.profile_dir(pid)
            }
        };
        dir.join(format!("{}.json", id))
    }

    fn prefs_file(&self) -> PathBuf {
        self.root.join("prefs.json")
    }

    /// List all dashboards for a given scope.
    pub fn list(&self, scope: DashboardScope, profile_id: Option<&str>) -> Result<Vec<ObdDashboard>> {
        let dir = match scope {
            DashboardScope::Global => self.global_dir(),
            DashboardScope::Profile => {
                let pid = profile_id.ok_or_else(|| Error::Profile("profile_id required for profile scope".into()))?;
                self.profile_dir(pid)
            }
        };

        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut dashboards = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(json) = std::fs::read_to_string(&path) {
                    if let Ok(dashboard) = serde_json::from_str::<ObdDashboard>(&json) {
                        dashboards.push(dashboard);
                    }
                }
            }
        }

        dashboards.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(dashboards)
    }

    /// Save a dashboard.
    pub fn save(&self, dashboard: &ObdDashboard) -> Result<()> {
        let dir = match dashboard.scope {
            DashboardScope::Global => self.global_dir(),
            DashboardScope::Profile => {
                let pid = dashboard.profile_id.as_deref().unwrap_or("default");
                self.profile_dir(pid)
            }
        };
        std::fs::create_dir_all(&dir)?;

        let path = self.dashboard_file(dashboard.scope, dashboard.profile_id.as_deref(), &dashboard.id);
        let json = serde_json::to_string_pretty(dashboard).map_err(Error::Json)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    /// Delete a dashboard.
    pub fn delete(&self, scope: DashboardScope, profile_id: Option<&str>, id: &str) -> Result<bool> {
        let path = self.dashboard_file(scope, profile_id, id);
        if !path.exists() {
            return Ok(false);
        }
        std::fs::remove_file(path)?;
        Ok(true)
    }

    /// Copy a dashboard to a different profile (or global → profile).
    pub fn copy_to_profile(&self, dashboard: &ObdDashboard, target_profile_id: &str) -> Result<ObdDashboard> {
        let mut copied = dashboard.clone();
        copied.id = format!("{}_{}", dashboard.id, target_profile_id);
        copied.scope = DashboardScope::Profile;
        copied.profile_id = Some(target_profile_id.to_string());
        
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        copied.updated_at_unix_ms = now;
        
        self.save(&copied)?;
        Ok(copied)
    }

    /// Get favorite dashboard ID (global preference).
    pub fn get_favorite(&self) -> Result<Option<String>> {
        let path = self.prefs_file();
        if !path.exists() {
            return Ok(None);
        }
        let json = std::fs::read_to_string(&path)?;
        let prefs: FavoritePrefs = serde_json::from_str(&json).map_err(Error::Json)?;
        Ok(prefs.favorite_dashboard_id)
    }

    /// Set favorite dashboard ID (global preference).
    pub fn set_favorite(&self, dashboard_id: Option<String>) -> Result<()> {
        let prefs = FavoritePrefs {
            favorite_dashboard_id: dashboard_id,
        };
        let json = serde_json::to_string_pretty(&prefs).map_err(Error::Json)?;
        std::fs::write(self.prefs_file(), json)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FavoritePrefs {
    favorite_dashboard_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obd_dashboard_store_operations() {
        let temp_dir = std::env::temp_dir().join(format!("ford_dash_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);

        let store = ObdDashboardStore::new(temp_dir.clone()).unwrap();

        // Create global dashboard
        let mut dashboard = ObdDashboard::new(
            "test_dash".into(),
            "Test Dashboard".into(),
            DashboardScope::Global,
            None,
        );
        dashboard.signals.push(ObdDashboardSignalKey {
            source: "pid".into(),
            id: "0C".into(),
        });

        store.save(&dashboard).unwrap();

        // List global
        let list = store.list(DashboardScope::Global, None).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Test Dashboard");

        // Set favorite
        store.set_favorite(Some("test_dash".into())).unwrap();
        assert_eq!(store.get_favorite().unwrap(), Some("test_dash".into()));

        // Copy to profile
        let copied = store.copy_to_profile(&dashboard, "focus_2017_eu").unwrap();
        assert_eq!(copied.scope, DashboardScope::Profile);
        assert_eq!(copied.profile_id, Some("focus_2017_eu".into()));

        let profile_list = store.list(DashboardScope::Profile, Some("focus_2017_eu")).unwrap();
        assert_eq!(profile_list.len(), 1);

        // Delete
        assert!(store.delete(DashboardScope::Global, None, "test_dash").unwrap());
        assert!(!store.delete(DashboardScope::Global, None, "test_dash").unwrap());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
