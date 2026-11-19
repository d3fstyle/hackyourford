//! High-level vehicle scan: probe modules, read DTCs, dump As-Built, live, discover.

use crate::as_built::{
    apply_param_edits, decode_params, fix_ford_as_built_groups, ParamEdit, ParamValue,
};
use crate::error::{Error, Result};
use crate::fixture::{load_builtin_fixture, DemoFixture};
use crate::live::{DiscoveredEcu, EcuDiscoverProgress, LiveSample, LiveSource, ModuleDtcGroup};
use crate::obd;
use crate::profile::{ModuleDef, VehicleProfile};
use crate::security::algo_by_id;
use crate::sniff::{self, CanFrame};
use crate::torque;
use crate::transport::{CanBus, DiagAdapter, MockTransport, StnTransport, Transport};
use crate::uds::{
    DidValue, Dtc, UdsClient, NRC_SECURITY_ACCESS_DENIED, SESSION_EXTENDED,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Ford-specific barometric pressure DID used by Torque BARO().
pub const FORD_BARO_DID: u16 = 0x1127;

/// A DID to poll in live mode with optional Torque equation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDidPin {
    pub module_id: String,
    pub did: u16,
    pub label: Option<String>,
    pub equation: Option<String>,
    pub unit: Option<String>,
}

/// Profile module or synthetic `HDR_XXX` from hex header.
fn resolve_poll_module(profile: Option<&VehicleProfile>, module_id: &str) -> Option<ModuleDef> {
    if let Some(p) = profile {
        if let Some(m) = p.module(module_id) {
            return Some(m.clone());
        }
    }
    // Synthetic header module (e.g. HDR_726 from Torque extended PIDs)
    if let Some(hex) = module_id.strip_prefix("HDR_") {
        if let Ok(tx) = u16::from_str_radix(hex, 16) {
            return Some(ModuleDef {
                id: module_id.to_string(),
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
    }
    None
}

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

/// One As-Built DID payload to write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsBuiltBlockWrite {
    pub did: u16,
    pub data: Vec<u8>,
}

/// Result of writing As-Built blocks (optional verify re-read).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsBuiltWriteResult {
    pub module_id: String,
    pub profile_id: String,
    pub written_dids: Vec<u16>,
    pub verified: Option<Vec<DidValue>>,
    pub checksum_fixed: bool,
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

    /// Write As-Built DID blocks for a profile module, then optionally re-read to verify.
    ///
    /// Only DIDs marked `as_built` in the profile are accepted. When `security_level` is set
    /// on the module, SecurityAccess is performed before writing. Otherwise a write that
    /// returns NRC 0x33 yields an actionable error.
    pub fn write_as_built(
        &mut self,
        module_id: &str,
        blocks: &[AsBuiltBlockWrite],
        fix_checksums: bool,
        verify: bool,
    ) -> Result<AsBuiltWriteResult> {
        let profile = self
            .profile
            .clone()
            .ok_or_else(|| Error::Profile("no profile selected".into()))?;
        let module = profile
            .module(module_id)
            .cloned()
            .ok_or_else(|| Error::Profile(format!("unknown module {module_id}")))?;

        let allowed: HashMap<u16, ()> = module
            .dids
            .iter()
            .filter(|d| d.as_built)
            .map(|d| (d.did, ()))
            .collect();
        if allowed.is_empty() {
            return Err(Error::Profile(format!(
                "module {module_id} has no as_built DIDs in profile"
            )));
        }
        if blocks.is_empty() {
            return Err(Error::Profile("no As-Built blocks to write".into()));
        }

        let mut prepared = Vec::with_capacity(blocks.len());
        let mut checksum_fixed = false;
        for block in blocks {
            if !allowed.contains_key(&block.did) {
                return Err(Error::Profile(format!(
                    "DID {:04X} is not an as_built DID for module {module_id}",
                    block.did
                )));
            }
            if block.data.is_empty() {
                return Err(Error::Profile(format!(
                    "DID {:04X}: empty data",
                    block.did
                )));
            }
            let mut data = block.data.clone();
            if fix_checksums && fix_ford_as_built_groups(&mut data) {
                checksum_fixed = true;
            }
            prepared.push((block.did, data));
        }

        let security_level = module.security_level;
        let security_algo_id = module
            .security_algo
            .clone()
            .unwrap_or_else(|| "experimental_xor_ff".into());

        let written_dids: Vec<u16> = prepared.iter().map(|(d, _)| *d).collect();

        self.with_module(&module, |t| {
            let mut uds = UdsClient::new(t);
            uds.diagnostic_session_control(SESSION_EXTENDED)?;

            if let Some(level) = security_level {
                let algo = algo_by_id(&security_algo_id);
                uds.unlock_security(level, algo.as_ref())?;
            }

            for (did, data) in &prepared {
                match uds.write_data_by_identifier(*did, data) {
                    Ok(()) => {}
                    Err(Error::UdsNegative {
                        nrc: NRC_SECURITY_ACCESS_DENIED,
                        ..
                    }) if security_level.is_none() => {
                        return Err(Error::Uds(format!(
                            "DID {did:04X}: securityAccessDenied — set security_level on module \
                             {module_id} in the profile (and a known security_algo) before writing"
                        )));
                    }
                    Err(Error::UdsNegative {
                        nrc: NRC_SECURITY_ACCESS_DENIED,
                        ..
                    }) => {
                        // Retry once after unlock if level was set but session dropped.
                        let algo = algo_by_id(&security_algo_id);
                        uds.unlock_security(security_level.unwrap(), algo.as_ref())?;
                        uds.write_data_by_identifier(*did, data)?;
                    }
                    Err(e) => return Err(e),
                }
            }

            let verified = if verify {
                let mut out = Vec::new();
                for (did, _) in &prepared {
                    out.push(uds.read_data_by_identifier(*did)?);
                }
                Some(out)
            } else {
                None
            };

            Ok(AsBuiltWriteResult {
                module_id: module_id.to_string(),
                profile_id: profile.id.clone(),
                written_dids: written_dids.clone(),
                verified,
                checksum_fixed,
            })
        })
    }

    /// Decode profile params against a fresh As-Built dump for `module_id`.
    pub fn decode_as_built_params(&mut self, module_id: &str) -> Result<Vec<ParamValue>> {
        let profile = self
            .profile
            .clone()
            .ok_or_else(|| Error::Profile("no profile selected".into()))?;
        let module = profile
            .module(module_id)
            .cloned()
            .ok_or_else(|| Error::Profile(format!("unknown module {module_id}")))?;
        if module.params.is_empty() {
            return Ok(Vec::new());
        }
        let dump = self.dump_as_built(module_id)?;
        decode_params(&dump.blocks, &module.params)
    }

    /// Read current As-Built blocks, apply param edits, write back.
    pub fn write_as_built_params(
        &mut self,
        module_id: &str,
        edits: &[ParamEdit],
        fix_checksums: bool,
        verify: bool,
    ) -> Result<AsBuiltWriteResult> {
        if edits.is_empty() {
            return Err(Error::Profile("no parameter edits to write".into()));
        }
        let profile = self
            .profile
            .clone()
            .ok_or_else(|| Error::Profile("no profile selected".into()))?;
        let module = profile
            .module(module_id)
            .cloned()
            .ok_or_else(|| Error::Profile(format!("unknown module {module_id}")))?;
        if module.params.is_empty() {
            return Err(Error::Profile(format!(
                "module {module_id} has no params in profile"
            )));
        }

        let dump = self.dump_as_built(module_id)?;
        let mut payloads: HashMap<u16, Vec<u8>> = dump
            .blocks
            .iter()
            .map(|b| (b.did, b.data.clone()))
            .collect();
        apply_param_edits(&mut payloads, edits, &module.params)?;

        let blocks: Vec<AsBuiltBlockWrite> = payloads
            .into_iter()
            .map(|(did, data)| AsBuiltBlockWrite { did, data })
            .collect();
        self.write_as_built(module_id, &blocks, fix_checksums, verify)
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

    /// One live poll cycle: Mode 01 PIDs + selected UDS DIDs (with Ford extended).
    /// Orders DIDs so BARO (0x1127) comes first when equations need it.
    pub fn poll_live_once(
        &mut self,
        pids: &[u8],
        dids: &[LiveDidPin],
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
                    raw_hex: s.hex.clone(),
                    decoded: s.decoded.clone(),
                    module_id: Some("PCM".into()),
                    label: Some(s.label.clone()),
                    value: s.value,
                    unit: s.unit.clone(),
                }),
                Err(_) => continue,
            }
        }

        // Order DIDs: BARO first, then equations using BARO(), then rest
        let mut ordered_dids = dids.to_vec();
        ordered_dids.sort_by_key(|pin| {
            if pin.did == FORD_BARO_DID {
                0
            } else if pin
                .equation
                .as_ref()
                .map(|eq| eq.to_uppercase().contains("BARO()"))
                .unwrap_or(false)
            {
                2
            } else {
                1
            }
        });

        let mut baro: Option<f64> = None;
        let profile = self.profile.clone();

        for pin in ordered_dids {
            let module = match resolve_poll_module(profile.as_ref(), &pin.module_id) {
                Some(m) => m,
                None => continue,
            };

            match self.with_module(&module, |t| {
                let mut uds = UdsClient::new(t);
                let _ = uds.diagnostic_session_control(SESSION_EXTENDED);
                uds.read_data_by_identifier(pin.did)
            }) {
                Ok(v) => {
                    let (numeric, display) = if let Some(ref eq) = pin.equation {
                        // Evaluate Torque equation
                        match torque::evaluate(eq, &v.data, baro) {
                            Ok(val) => {
                                // Update baro if this is the BARO DID
                                if pin.did == FORD_BARO_DID {
                                    baro = Some(val);
                                }
                                let unit = pin.unit.as_deref().unwrap_or("");
                                let display = torque::format_value(val, unit);
                                (Some(val), display)
                            }
                            Err(_) => {
                                // Fallback to raw numeric or hex
                                let num = obd::raw_numeric_did(&v.data);
                                let disp = v.ascii.unwrap_or_else(|| v.hex.clone());
                                (num, disp)
                            }
                        }
                    } else {
                        // No equation: try raw numeric or use ASCII/hex
                        let num = obd::raw_numeric_did(&v.data);
                        let disp = v.ascii.unwrap_or_else(|| v.hex.clone());
                        (num, disp)
                    };

                    samples.push(LiveSample {
                        ts_ms,
                        source: LiveSource::Did,
                        id: u32::from(pin.did),
                        raw_hex: v.hex,
                        decoded: Some(display),
                        module_id: Some(pin.module_id.clone()),
                        label: pin.label.or_else(|| Some(format!("DID 0x{:04X}", pin.did))),
                        value: numeric,
                        unit: pin.unit,
                    });
                }
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

        let write = session
            .write_as_built(
                "BCM",
                &[AsBuiltBlockWrite {
                    did: 0xDE00,
                    data: vec![0xAA, 0xBB, 0xCC, 0xDD, 0x01, 0x02, 0x03, 0x04],
                }],
                false,
                true,
            )
            .unwrap();
        assert_eq!(write.written_dids, vec![0xDE00]);
        assert!(write.verified.is_some());

        // Looping fixture: scan again yields the same online modules
        let again = session.scan_modules().unwrap();
        assert_eq!(
            again.iter().filter(|m| m.online).count(),
            results.iter().filter(|m| m.online).count()
        );
    }

    #[test]
    fn write_as_built_rejects_non_asbuilt_did() {
        let mut session = ScannerSession::connect_demo("focus_2017_eu_demo").unwrap();
        session.set_profile(builtin_profile_by_id("focus_2017_eu").unwrap());
        let err = session
            .write_as_built(
                "BCM",
                &[AsBuiltBlockWrite {
                    did: 0xF190,
                    data: vec![1, 2, 3],
                }],
                false,
                false,
            )
            .unwrap_err();
        assert!(err.to_string().contains("not an as_built"));
    }

    #[test]
    fn write_as_built_with_security_level() {
        let mut session = ScannerSession::connect_demo("focus_2017_eu_demo").unwrap();
        let mut profile = builtin_profile_by_id("focus_2017_eu").unwrap();
        let bcm = profile.modules.iter_mut().find(|m| m.id == "BCM").unwrap();
        bcm.security_level = Some(0x01);
        bcm.security_algo = Some("experimental_xor_ff".into());
        session.set_profile(profile);

        let write = session
            .write_as_built(
                "BCM",
                &[AsBuiltBlockWrite {
                    did: 0xDE01,
                    data: vec![0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18],
                }],
                false,
                false,
            )
            .unwrap();
        assert_eq!(write.written_dids, vec![0xDE01]);
    }

    #[test]
    fn tourneo_profile_write_path() {
        let mut session = ScannerSession::connect_demo("focus_2017_eu_demo").unwrap();
        session.set_profile(builtin_profile_by_id("tourneo_connect_2017").unwrap());
        let write = session
            .write_as_built(
                "BCM",
                &[AsBuiltBlockWrite {
                    did: 0xDE00,
                    data: vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
                }],
                false,
                false,
            )
            .unwrap();
        assert_eq!(write.module_id, "BCM");
        assert_eq!(write.profile_id, "tourneo_connect_2017");
    }

    #[test]
    fn demo_decode_and_write_params() {
        let mut session = ScannerSession::connect_demo("focus_2017_eu_demo").unwrap();
        session.set_profile(builtin_profile_by_id("focus_2017_eu").unwrap());

        let values = session.decode_as_built_params("BCM").unwrap();
        assert!(!values.is_empty());
        let drl = values.iter().find(|v| v.id == "drl_enable").unwrap();
        assert_eq!(drl.value, 1);
        let mode = values
            .iter()
            .find(|v| v.id == "approach_light_mode")
            .unwrap();
        assert_eq!(mode.value, 3);

        let result = session
            .write_as_built_params(
                "BCM",
                &[ParamEdit {
                    id: "drl_enable".into(),
                    value: 0,
                }],
                false,
                true,
            )
            .unwrap();
        assert!(result.written_dids.contains(&0xDE00));
    }
}
