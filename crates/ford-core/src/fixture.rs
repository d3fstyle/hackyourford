//! Built-in demo fixtures: scripted AT/UDS responses for hardware-free UI testing.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

const FOCUS_2017_EU_DEMO: &str = include_str!("../fixtures/focus_2017_eu_demo.json");

/// Adapter identity embedded in a fixture (applied on demo connect).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureAdapter {
    pub identity: String,
    pub is_stn: bool,
    pub voltage: Option<f32>,
}

/// One command → response rule. `match` is a prefix or exact command (case-insensitive).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureScript {
    #[serde(rename = "match")]
    pub match_prefix: String,
    pub response: String,
}

/// Full demo fixture loaded from JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoFixture {
    pub id: String,
    pub name: String,
    pub profile_hint: String,
    pub adapter: FixtureAdapter,
    pub scripts: Vec<FixtureScript>,
}

/// Summary for UI listing (no scripts).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoFixtureInfo {
    pub id: String,
    pub name: String,
    pub profile_hint: String,
}

fn builtin_raw() -> &'static [(&'static str, &'static str)] {
    &[("focus_2017_eu_demo", FOCUS_2017_EU_DEMO)]
}

/// List built-in demo fixtures available without hardware.
pub fn list_builtin_fixtures() -> Result<Vec<DemoFixtureInfo>> {
    builtin_raw()
        .iter()
        .map(|(_, raw)| {
            let f: DemoFixture = serde_json::from_str(raw)?;
            Ok(DemoFixtureInfo {
                id: f.id,
                name: f.name,
                profile_hint: f.profile_hint,
            })
        })
        .collect()
}

/// Load a built-in fixture by id.
pub fn load_builtin_fixture(id: &str) -> Result<DemoFixture> {
    for (fid, raw) in builtin_raw() {
        if *fid == id {
            return Ok(serde_json::from_str(raw)?);
        }
    }
    Err(Error::Fixture(format!("unknown demo fixture: {id}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_focus_demo() {
        let f = load_builtin_fixture("focus_2017_eu_demo").unwrap();
        assert_eq!(f.profile_hint, "focus_2017_eu");
        assert!(f.adapter.is_stn);
        assert!(!f.scripts.is_empty());
    }

    #[test]
    fn lists_at_least_one() {
        let list = list_builtin_fixtures().unwrap();
        assert!(list.iter().any(|i| i.id == "focus_2017_eu_demo"));
    }

    #[test]
    fn unknown_fixture_errors() {
        assert!(load_builtin_fixture("nope").is_err());
    }
}
