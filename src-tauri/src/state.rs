use crate::worker::{LiveConfig, SnapshotWhich, WorkerCmd, WorkerHandle, WorkerMode};
use ford_core::{
    AsBuiltDump, CanBus, ConnectionStatus, DiscoveredEcu, Dtc, ModuleDtcGroup, ModuleScanResult,
    PatternCandidate, VehicleProfile, DidValue,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

pub struct AppState {
    pub worker: WorkerHandle,
}

impl AppState {
    fn call<T>(&self, build: impl FnOnce(mpsc::Sender<Result<T, String>>) -> WorkerCmd) -> Result<T, String> {
        let (tx, rx) = mpsc::channel();
        self.worker
            .tx
            .send(build(tx))
            .map_err(|_| "session worker unavailable".to_string())?;
        rx.recv_timeout(Duration::from_secs(120))
            .map_err(|_| "session worker timed out".to_string())?
    }

    pub fn connect(&self, port: &str, baud: u32) -> Result<ConnectionStatus, String> {
        self.call(|reply| WorkerCmd::Connect {
            port: port.to_string(),
            baud,
            reply,
        })
    }

    pub fn connect_demo(&self, fixture_id: &str) -> Result<ConnectionStatus, String> {
        self.call(|reply| WorkerCmd::ConnectDemo {
            fixture_id: fixture_id.to_string(),
            reply,
        })
    }

    pub fn disconnect(&self) -> Result<(), String> {
        self.call(|reply| WorkerCmd::Disconnect { reply })
    }

    pub fn status(&self) -> Result<Option<ConnectionStatus>, String> {
        self.call(|reply| WorkerCmd::Status { reply })
    }

    pub fn set_profile(&self, profile: VehicleProfile) -> Result<ConnectionStatus, String> {
        self.call(|reply| WorkerCmd::SetProfile { profile, reply })
    }

    pub fn read_vin(&self) -> Result<Option<String>, String> {
        self.call(|reply| WorkerCmd::ReadVin { reply })
    }

    pub fn scan_modules(&self) -> Result<Vec<ModuleScanResult>, String> {
        self.call(|reply| WorkerCmd::ScanModules { reply })
    }

    pub fn read_dtcs(&self, module_id: &str) -> Result<Vec<Dtc>, String> {
        self.call(|reply| WorkerCmd::ReadDtcs {
            module_id: module_id.to_string(),
            reply,
        })
    }

    pub fn read_all_dtcs(&self) -> Result<Vec<ModuleDtcGroup>, String> {
        self.call(|reply| WorkerCmd::ReadAllDtcs { reply })
    }

    pub fn clear_dtcs(&self, module_id: &str) -> Result<(), String> {
        self.call(|reply| WorkerCmd::ClearDtcs {
            module_id: module_id.to_string(),
            reply,
        })
    }

    pub fn dump_as_built(&self, module_id: &str) -> Result<AsBuiltDump, String> {
        self.call(|reply| WorkerCmd::DumpAsBuilt {
            module_id: module_id.to_string(),
            reply,
        })
    }

    pub fn discover_ecus(
        &self,
        bus: CanBus,
        from_id: u16,
        to_id: u16,
    ) -> Result<Vec<DiscoveredEcu>, String> {
        // Discovery can take a long time — use longer timeout via dedicated channel
        let (tx, rx) = mpsc::channel();
        self.worker
            .tx
            .send(WorkerCmd::DiscoverEcus {
                bus,
                from_id,
                to_id,
                reply: tx,
            })
            .map_err(|_| "session worker unavailable".to_string())?;
        rx.recv_timeout(Duration::from_secs(600))
            .map_err(|_| "discover timed out".to_string())?
    }

    pub fn start_live(&self, config: LiveConfig) -> Result<(), String> {
        self.call(|reply| WorkerCmd::StartLive { config, reply })
    }

    pub fn list_supported_pids(&self) -> Result<Vec<u8>, String> {
        self.call(|reply| WorkerCmd::ListSupportedPids { reply })
    }

    pub fn start_sniff(&self, bus: CanBus) -> Result<(), String> {
        self.call(|reply| WorkerCmd::StartSniff { bus, reply })
    }

    pub fn mark_press(&self) -> Result<(), String> {
        self.call(|reply| WorkerCmd::MarkPress { reply })
    }

    pub fn analyze_pattern(&self) -> Result<Vec<PatternCandidate>, String> {
        self.call(|reply| WorkerCmd::AnalyzePattern { reply })
    }

    pub fn start_did_sweep(
        &self,
        module_id: &str,
        from_did: u16,
        to_did: u16,
    ) -> Result<Vec<DidValue>, String> {
        let (tx, rx) = mpsc::channel();
        self.worker
            .tx
            .send(WorkerCmd::StartDidSweep {
                module_id: module_id.to_string(),
                from_did,
                to_did,
                reply: tx,
            })
            .map_err(|_| "session worker unavailable".to_string())?;
        rx.recv_timeout(Duration::from_secs(600))
            .map_err(|_| "did sweep timed out".to_string())?
    }

    pub fn did_snapshot(
        &self,
        module_id: &str,
        from_did: u16,
        to_did: u16,
        which: SnapshotWhich,
    ) -> Result<HashMap<u16, String>, String> {
        let (tx, rx) = mpsc::channel();
        self.worker
            .tx
            .send(WorkerCmd::DidSnapshot {
                module_id: module_id.to_string(),
                from_did,
                to_did,
                which,
                reply: tx,
            })
            .map_err(|_| "session worker unavailable".to_string())?;
        rx.recv_timeout(Duration::from_secs(600))
            .map_err(|_| "snapshot timed out".to_string())?
    }

    pub fn diff_did_snapshots(&self) -> Result<Vec<(u16, String, String)>, String> {
        self.call(|reply| WorkerCmd::DiffDidSnapshots { reply })
    }

    pub fn save_live_session(&self, path: PathBuf) -> Result<(), String> {
        self.call(|reply| WorkerCmd::SaveLiveSession { path, reply })
    }

    pub fn get_live_samples(&self) -> Result<Vec<ford_core::LiveSample>, String> {
        self.call(|reply| WorkerCmd::GetLiveSamples { reply })
    }

    pub fn stop_activity(&self) -> Result<WorkerMode, String> {
        self.worker.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        self.call(|reply| WorkerCmd::StopActivity { reply })
    }
}
