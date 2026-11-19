//! Live session sample types and JSON export helpers.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveSource {
    Did,
    Pid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveSample {
    pub ts_ms: u64,
    pub source: LiveSource,
    pub id: u32,
    pub raw_hex: String,
    pub decoded: Option<String>,
    pub module_id: Option<String>,
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveSessionFile {
    pub started_at_unix_ms: u128,
    pub profile_id: Option<String>,
    pub port: String,
    pub samples: Vec<LiveSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDtcGroup {
    pub module_id: String,
    pub module_name: String,
    pub tx_id: u16,
    pub bus: crate::transport::CanBus,
    pub online: bool,
    pub error: Option<String>,
    pub dtcs: Vec<crate::uds::Dtc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcuDiscoverProgress {
    pub bus: crate::transport::CanBus,
    pub current_id: u16,
    pub from_id: u16,
    pub to_id: u16,
    pub found: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredEcu {
    pub tx_id: u16,
    pub rx_id: u16,
    pub bus: crate::transport::CanBus,
    pub online: bool,
    pub profile_id: Option<String>,
    pub name: Option<String>,
    pub identity: Vec<crate::uds::DidValue>,
    pub error: Option<String>,
}
