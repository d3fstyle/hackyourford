//! High-level vehicle scan: probe modules, read DTCs, dump As-Built, live, discover.

use crate::error::{Error, Result};
use crate::fixture::{load_builtin_fixture, DemoFixture};
use crate::live::{DiscoveredEcu, EcuDiscoverProgress, LiveSample, LiveSource, ModuleDtcGroup};
use crate::obd;
use crate::profile::{ModuleDef, VehicleProfile};
use crate::sniff::{self, CanFrame};
use crate::transport::{CanBus, DiagAdapter, MockTransport, StnTransport, Transport};
use crate::uds::{DidValue, Dtc, UdsClient, SESSION_EXTENDED};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleScanResult {
    pub id: String,
    pub name: String,
    pub tx_id: u16,
    pub bus: CanBus,
    pub online: bool,
    pub error: Option<String>,
    pub identity: Vec<DidValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsBuiltDump {
    pub module_id: String,
    pub module_name: String,
    pub profile_id: String,
    pub blocks: Vec<DidValue>,
    pub exported_at_unix_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatus {
    pub port: String,
    pub adapter: crate::transport::AdapterInfo,
    pub profile_id: Option<String>,
    pub vin: Option<String>,
    pub ms_can_capable: bool,
}

/// Session owning the serial (or demo) adapter and optional selected profile.
pub struct ScannerSession {
    port_name: String,
    transport: Box<dyn DiagAdapter>,
    profile: Option<VehicleProfile>,
    vin: Option<String>,
}

impl ScannerSession {
    pub fn connect(port: &str, baud: u32) -> Result<Self> {
        let mut transport = StnTransport::open(port, baud)?;
        transport.set_timeout(Duration::from_secs(5));
        let _adapter = transport.initialize()?;
        Ok(Self {
            port_name: port.to_string(),
            transport: Box::new(transport),
            profile: None,
            vin: None,
        })
    }

    /// Connect using a built-in demo fixture (no serial hardware).
    pub fn connect_demo(fixture_id: &str) -> Result<Self> {
        let fixture = load_builtin_fixture(fixture_id)?;
        Self::from_fixture(fixture)
    }

    /// Connect from an already-loaded fixture (tests / future user imports).
    pub fn from_fixture(fixture: DemoFixture) -> Result<Self> {
        let port_name = format!("demo:{}", fixture.id);
        let mut transport = MockTransport::from_fixture(&fixture);
        transport.set_timeout(Duration::from_secs(5));
        let _adapter = transport.initialize()?;
        Ok(Self {
            port_name,
            transport: Box::new(transport),
            profile: None,
            vin: None,
        })
    }

    pub fn status(&self) -> Result<ConnectionStatus> {
        let adapter = self
            .transport
            .adapter_info()
            .cloned()
            .ok_or(Error::NotConnected)?;
        Ok(ConnectionStatus {
            port: self.port_name.clone(),
            ms_can_capable: adapter.is_stn,
            adapter,
            profile_id: self.profile.as_ref().map(|p| p.id.clone()),
            vin: self.vin.clone(),
        })
    }

    pub fn set_profile(&mut self, profile: VehicleProfile) {
        self.profile = Some(profile);
    }

    pub fn profile(&self) -> Option<&VehicleProfile> {
        self.profile.as_ref()
    }

    fn with_module<T>(
        &mut self,
        module: &ModuleDef,
        f: impl FnOnce(&mut dyn Transport) -> Result<T>,
    ) -> Result<T> {
        self.transport.select_bus(module.bus)?;
        self.transport.set_header(module.tx_id as u32)?;
        f(self.transport.as_mut())
    }

    /// Probe each module: extended session + read first DID or tester present.
    pub fn scan_modules(&mut self) -> Result<Vec<ModuleScanResult>> {
        let profile = self
            .profile
            .clone()
            .ok_or_else(|| Error::Profile("no profile selected".into()))?;

        let mut results = Vec::new();
        for module in &profile.modules {
            let mut result = ModuleScanResult {
                id: module.id.clone(),
                name: module.name.clone(),
                tx_id: module.tx_id,
                bus: module.bus,
                online: false,
                error: None,
                identity: Vec::new(),
            };

            if module.bus == CanBus::Ms
                && !self
                    .transport
                    .adapter_info()
                    .map(|i| i.is_stn)
                    .unwrap_or(false)
            {
                result.error = Some(
                    "MS-CAN module skipped: adapter does not look like STN/OBDLink (use MX+ or enable physical MS-CAN switch)".into(),
                );
                results.push(result);
                continue;
            }

            let probe = self.with_module(module, |t| {
                let mut uds = UdsClient::new(t);
                let _ = uds.diagnostic_session_control(SESSION_EXTENDED);
                let mut identity = Vec::new();
                if let Some(did) = module.dids.first() {
                    match uds.read_data_by_identifier(did.did) {
                        Ok(v) => identity.push(v),
                        Err(_) => {
                            uds.tester_present()?;
                        }
                    }
                } else {
                    uds.tester_present()?;
                }
                // Try a few more identity DIDs without failing the probe
                for did in module.dids.iter().skip(1).take(2) {
                    if let Ok(v) = uds.read_data_by_identifier(did.did) {
                        identity.push(v);
                    }
                }
                Ok(identity)
            });

            match probe {
                Ok(identity) => {
                    result.online = true;
                    // Capture VIN from PCM
                    if module.id == "PCM" {
                        if let Some(v) = identity.iter().find(|d| d.did == 0xF190) {
                            self.vin = v.ascii.clone().or_else(|| Some(v.hex.clone()));
                        }
                    }
                    result.identity = identity;
                }
                Err(e) => {
                    result.online = false;
                    result.error = Some(e.to_string());
                }
            }
            results.push(result);
        }
        Ok(results)
    }

    pub fn read_module_dtcs(&mut self, module_id: &str) -> Result<Vec<Dtc>> {
        let module = self
            .profile
            .as_ref()
            .and_then(|p| p.module(module_id).cloned())
            .ok_or_else(|| Error::Profile(format!("unknown module {module_id}")))?;

        self.with_module(&module, |t| {
            let mut uds = UdsClient::new(t);
            let _ = uds.diagnostic_session_control(SESSION_EXTENDED);
            uds.read_dtcs()
        })
    }

    pub fn dump_as_built(&mut self, module_id: &str) -> Result<AsBuiltDump> {
        let profile = self
            .profile
            .clone()
            .ok_or_else(|| Error::Profile("no profile selected".into()))?;
        let module = profile
            .module(module_id)
            .cloned()
            .ok_or_else(|| Error::Profile(format!("unknown module {module_id}")))?;

        let dids: Vec<_> = module
            .dids
            .iter()
            .filter(|d| d.as_built)
            .cloned()
            .collect();

        if dids.is_empty() {
            return Err(Error::Profile(format!(
                "module {module_id} has no as_built DIDs in profile"
            )));
        }

        let blocks = self.with_module(&module, |t| {
            let mut uds = UdsClient::new(t);
            let _ = uds.diagnostic_session_control(SESSION_EXTENDED);
            let mut blocks = Vec::new();
            for did in &dids {
                match uds.read_data_by_identifier(did.did) {
                    Ok(v) => blocks.push(v),
                    Err(e) => {
                        tracing::warn!("DID {:04X} read failed: {e}", did.did);
                    }
                }
            }
            Ok(blocks)
        })?;

        Ok(AsBuiltDump {
            module_id: module.id,
            module_name: module.name,
            profile_id: profile.id,
            blocks,
            exported_at_unix_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0),
        })
    }

    /// Read VIN from PCM (HS-CAN 0x7E0) without requiring a full scan.
    pub fn read_vin(&mut self) -> Result<Option<String>> {
        self.transport.select_bus(CanBus::Hs)?;
        self.transport.set_header(0x7E0)?;
        let mut uds = UdsClient::new(self.transport.as_mut());
        let _ = uds.diagnostic_session_control(SESSION_EXTENDED);
        match uds.read_data_by_identifier(0xF190) {
            Ok(v) => {
                let vin = v.ascii.or(Some(v.hex));
                self.vin = vin.clone();
                Ok(vin)
            }
            Err(_) => Ok(None),
        }
    }

    pub fn clear_module_dtcs(&mut self, module_id: &str) -> Result<()> {
        let module = self
            .profile
            .as_ref()
            .and_then(|p| p.module(module_id).cloned())
            .ok_or_else(|| Error::Profile(format!("unknown module {module_id}")))?;

        self.with_module(&module, |t| {
            let mut uds = UdsClient::new(t);
            let _ = uds.diagnostic_session_control(SESSION_EXTENDED);
            uds.clear_dtcs()
        })
    }

    pub fn read_all_dtcs(&mut self) -> Result<Vec<ModuleDtcGroup>> {
        let profile = self
            .profile
            .clone()
            .ok_or_else(|| Error::Profile("no profile selected".into()))?;
        let mut groups = Vec::new();
        for module in &profile.modules {
            let mut group = ModuleDtcGroup {
                module_id: module.id.clone(),
                module_name: module.name.clone(),
                tx_id: module.tx_id,
                bus: module.bus,
                online: false,
                error: None,
                dtcs: Vec::new(),
            };
            if module.bus == CanBus::Ms
                && !self
                    .transport
                    .adapter_info()
                    .map(|i| i.is_stn)
                    .unwrap_or(false)
            {
                group.error = Some("MS-CAN skipped (non-STN adapter)".into());
                groups.push(group);
                continue;
            }
            match self.with_module(module, |t| {
                let mut uds = UdsClient::new(t);
                let _ = uds.diagnostic_session_control(SESSION_EXTENDED);
                uds.read_dtcs()
            }) {
                Ok(dtcs) => {
                    group.online = true;
                    group.dtcs = dtcs;
                }
                Err(e) => {
                    group.error = Some(e.to_string());
                }
            }
            groups.push(group);
        }
        Ok(groups)
    }

    /// Brute-force probe of 11-bit diagnostic IDs on one bus.
    pub fn discover_ecus(
        &mut self,
        bus: CanBus,
        from_id: u16,
        to_id: u16,
        cancel: &AtomicBool,
        mut on_progress: impl FnMut(EcuDiscoverProgress),
    ) -> Result<Vec<DiscoveredEcu>> {
        if bus == CanBus::Ms
            && !self
                .transport
                .adapter_info()
                .map(|i| i.is_stn)
                .unwrap_or(false)
        {
            return Err(Error::Adapter(
                "MS-CAN discovery requires STN/OBDLink adapter".into(),
            ));
        }

        self.transport.select_bus(bus)?;
        let profile = self.profile.clone();
        let mut found = Vec::new();
        let end = to_id.max(from_id);

        for id in from_id..=end {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            on_progress(EcuDiscoverProgress {
                bus,
                current_id: id,
                from_id,
                to_id: end,
                found: found.len(),
            });

            self.transport.set_header(id as u32)?;
            // Short timeout for probes
            self.transport.set_timeout(Duration::from_millis(400));
            let probe = {
                let mut uds = UdsClient::new(self.transport.as_mut());
                match uds.tester_present() {
                    Ok(()) => Ok(None),
                    Err(_) => uds.read_data_by_identifier(0xF190).map(Some),
                }
            };
            self.transport.set_timeout(Duration::from_secs(5));

            let Ok(maybe_vin) = probe else {
                continue;
            };

            let mut identity = Vec::new();
            if let Some(v) = maybe_vin {
                identity.push(v);
            } else {
                let mut uds = UdsClient::new(self.transport.as_mut());
                if let Ok(v) = uds.read_data_by_identifier(0xF190) {
                    identity.push(v);
                }
                if let Ok(v) = uds.read_data_by_identifier(0xF188) {
                    identity.push(v);
                }
            }

            let (profile_id, name) = profile
                .as_ref()
                .and_then(|p| {
                    p.modules
                        .iter()
                        .find(|m| m.tx_id == id && m.bus == bus)
                        .map(|m| (Some(m.id.clone()), Some(m.name.clone())))
                })
                .unwrap_or((None, None));

            found.push(DiscoveredEcu {
                tx_id: id,
                rx_id: id | 0x08,
                bus,
                online: true,
                profile_id,
                name,
                identity,
                error: None,
            });
        }
        Ok(found)
    }

    /// One live poll cycle: Mode 01 PIDs + selected UDS DIDs.
    pub fn poll_live_once(
        &mut self,
        pids: &[u8],
        dids: &[(String, u16, Option<String>)], // module_id, did, label
    ) -> Result<Vec<LiveSample>> {
        let ts_ms = now_ms();
        let mut samples = Vec::new();

        // Mode 01 via PCM header on HS-CAN
        self.transport.select_bus(CanBus::Hs)?;
        self.transport.set_header(0x7E0)?;
        for &pid in pids {
            match obd::read_pid(self.transport.as_mut(), pid) {
                Ok(s) => samples.push(LiveSample {
                    ts_ms,
                    source: LiveSource::Pid,
                    id: u32::from(pid),
                    raw_hex: s.hex,
                    decoded: s.decoded,
                    module_id: Some("PCM".into()),
                    label: Some(format!("PID {pid:02X}")),
                }),
                Err(_) => continue,
            }
        }

        let profile = self.profile.clone();
        for (module_id, did, label) in dids {
            let Some(module) = profile.as_ref().and_then(|p| p.module(module_id)) else {
                continue;
            };
            let label = label.clone();
            let did = *did;
            match self.with_module(module, |t| {
                let mut uds = UdsClient::new(t);
                let _ = uds.diagnostic_session_control(SESSION_EXTENDED);
                uds.read_data_by_identifier(did)
            }) {
                Ok(v) => samples.push(LiveSample {
                    ts_ms,
                    source: LiveSource::Did,
                    id: u32::from(did),
                    raw_hex: v.hex,
                    decoded: v.ascii,
                    module_id: Some(module_id.clone()),
                    label,
                }),
                Err(_) => continue,
            }
        }
        Ok(samples)
    }

    pub fn list_supported_obd_pids(&mut self) -> Result<Vec<u8>> {
        self.transport.select_bus(CanBus::Hs)?;
        self.transport.set_header(0x7E0)?;
        obd::list_supported_pids(self.transport.as_mut())
    }

    pub fn start_can_sniff(&mut self, bus: CanBus) -> Result<()> {
        self.transport.select_bus(bus)?;
        self.transport.clear_receive_filter()?;
        let _ = self.transport.command("ATH1");
        self.transport.start_monitor()
    }

    pub fn read_sniff_chunk(&mut self, max_wait: Duration) -> Result<Vec<CanFrame>> {
        let chunk = self.transport.read_monitor_chunk(max_wait)?;
        Ok(sniff::parse_monitor_buffer(&chunk, now_ms()))
    }

    pub fn stop_can_sniff(&mut self) -> Result<()> {
        self.transport.stop_monitor()
    }

    /// Sweep DIDs on a module; returns successful reads.
    pub fn sweep_dids(
        &mut self,
        module_id: &str,
        from_did: u16,
        to_did: u16,
        cancel: &AtomicBool,
        mut on_found: impl FnMut(DidValue),
    ) -> Result<Vec<DidValue>> {
        let module = self
            .profile
            .as_ref()
            .and_then(|p| p.module(module_id).cloned())
            .ok_or_else(|| Error::Profile(format!("unknown module {module_id}")))?;

        let end = to_did.max(from_did);
        let mut found = Vec::new();
        self.transport.select_bus(module.bus)?;
        self.transport.set_header(module.tx_id as u32)?;
        {
            let mut uds = UdsClient::new(self.transport.as_mut());
            let _ = uds.diagnostic_session_control(SESSION_EXTENDED);
        }

        for did in from_did..=end {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            self.transport.set_timeout(Duration::from_millis(350));
            let result = {
                let mut uds = UdsClient::new(self.transport.as_mut());
                uds.read_data_by_identifier(did)
            };
            self.transport.set_timeout(Duration::from_secs(5));
            if let Ok(v) = result {
                on_found(v.clone());
                found.push(v);
            }
        }
        Ok(found)
    }

    pub fn snapshot_did_range(
        &mut self,
        module_id: &str,
        from_did: u16,
        to_did: u16,
        cancel: &AtomicBool,
    ) -> Result<HashMap<u16, String>> {
        let values = self.sweep_dids(module_id, from_did, to_did, cancel, |_| {})?;
        Ok(values.into_iter().map(|v| (v.did, v.hex)).collect())
    }

    pub fn port_name(&self) -> &str {
        &self.port_name
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::builtin_profile_by_id;

    #[test]
    fn demo_scan_dtc_asbuilt_replay() {
        let mut session = ScannerSession::connect_demo("focus_2017_eu_demo").unwrap();
        let profile = builtin_profile_by_id("focus_2017_eu").unwrap();
        session.set_profile(profile);

        let results = session.scan_modules().unwrap();
        let pcm = results.iter().find(|m| m.id == "PCM").unwrap();
        assert!(pcm.online, "PCM should be online in demo: {:?}", pcm.error);
        assert!(session.vin.as_ref().is_some_and(|v| v.contains("WF0")));

        let bcm = results.iter().find(|m| m.id == "BCM").unwrap();
        assert!(bcm.online, "BCM should be online: {:?}", bcm.error);

        let dtcs = session.read_module_dtcs("PCM").unwrap();
        assert!(!dtcs.is_empty());
        assert_eq!(dtcs[0].code, "P030100");

        let dump = session.dump_as_built("BCM").unwrap();
        assert_eq!(dump.blocks.len(), 4);

        // Looping fixture: scan again yields the same online modules
        let again = session.scan_modules().unwrap();
        assert_eq!(
            again.iter().filter(|m| m.online).count(),
            results.iter().filter(|m| m.online).count()
        );
    }
}
