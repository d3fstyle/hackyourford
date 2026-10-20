//! Session worker thread: owns the serial adapter and emits Tauri events.

use ford_core::{
    correlate_press_pattern, diff_did_snapshots, AsBuiltDump, CanBus, CanFrame, ConnectionStatus,
    DiscoveredEcu, Dtc, EcuDiscoverProgress, LiveSample, LiveSessionFile, LiveSource,
    ModuleDtcGroup, ModuleScanResult, PatternCandidate, ScannerSession, VehicleProfile, DidValue,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerMode {
    Idle,
    EcuDiscover,
    Live,
    CanSniff,
    DidSweep,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveConfig {
    pub pids: Vec<u8>,
    /// (module_id, did, optional label)
    pub dids: Vec<(String, u16, Option<String>)>,
    pub interval_ms: u64,
}

pub enum WorkerCmd {
    Connect {
        port: String,
        baud: u32,
        reply: Sender<Result<ConnectionStatus, String>>,
    },
    ConnectDemo {
        fixture_id: String,
        reply: Sender<Result<ConnectionStatus, String>>,
    },
    Disconnect {
        reply: Sender<Result<(), String>>,
    },
    Status {
        reply: Sender<Result<Option<ConnectionStatus>, String>>,
    },
    SetProfile {
        profile: VehicleProfile,
        reply: Sender<Result<ConnectionStatus, String>>,
    },
    ReadVin {
        reply: Sender<Result<Option<String>, String>>,
    },
    ScanModules {
        reply: Sender<Result<Vec<ModuleScanResult>, String>>,
    },
    ReadDtcs {
        module_id: String,
        reply: Sender<Result<Vec<Dtc>, String>>,
    },
    ReadAllDtcs {
        reply: Sender<Result<Vec<ModuleDtcGroup>, String>>,
    },
    ClearDtcs {
        module_id: String,
        reply: Sender<Result<(), String>>,
    },
    DumpAsBuilt {
        module_id: String,
        reply: Sender<Result<AsBuiltDump, String>>,
    },
    DiscoverEcus {
        bus: CanBus,
        from_id: u16,
        to_id: u16,
        reply: Sender<Result<Vec<DiscoveredEcu>, String>>,
    },
    StartLive {
        config: LiveConfig,
        reply: Sender<Result<(), String>>,
    },
    ListSupportedPids {
        reply: Sender<Result<Vec<u8>, String>>,
    },
    StartSniff {
        bus: CanBus,
        reply: Sender<Result<(), String>>,
    },
    MarkPress {
        reply: Sender<Result<(), String>>,
    },
    AnalyzePattern {
        reply: Sender<Result<Vec<PatternCandidate>, String>>,
    },
    StartDidSweep {
        module_id: String,
        from_did: u16,
        to_did: u16,
        reply: Sender<Result<Vec<DidValue>, String>>,
    },
    DidSnapshot {
        module_id: String,
        from_did: u16,
        to_did: u16,
        which: SnapshotWhich,
        reply: Sender<Result<HashMap<u16, String>, String>>,
    },
    DiffDidSnapshots {
        reply: Sender<Result<Vec<(u16, String, String)>, String>>,
    },
    SaveLiveSession {
        path: PathBuf,
        reply: Sender<Result<(), String>>,
    },
    GetLiveSamples {
        reply: Sender<Result<Vec<LiveSample>, String>>,
    },
    StopActivity {
        reply: Sender<Result<WorkerMode, String>>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum SnapshotWhich {
    Before,
    After,
}

struct WorkerState {
    session: Option<ScannerSession>,
    mode: WorkerMode,
    cancel: Arc<AtomicBool>,
    live_config: Option<LiveConfig>,
    live_samples: Vec<LiveSample>,
    live_started_ms: u128,
    sniff_frames: Vec<CanFrame>,
    press_marks: Vec<u64>,
    press_windows: Vec<(u64, u64)>,
    snap_before: HashMap<u16, String>,
    snap_after: HashMap<u16, String>,
    app: AppHandle,
}

#[derive(Clone)]
pub struct WorkerHandle {
    pub tx: SyncSender<WorkerCmd>,
    pub cancel: Arc<AtomicBool>,
}

pub fn spawn_worker(app: AppHandle) -> WorkerHandle {
    let (tx, rx) = mpsc::sync_channel::<WorkerCmd>(64);
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_thread = cancel.clone();
    thread::Builder::new()
        .name("forddiag-session".into())
        .spawn(move || {
            let mut state = WorkerState {
                session: None,
                mode: WorkerMode::Idle,
                cancel: cancel_thread,
                live_config: None,
                live_samples: Vec::new(),
                live_started_ms: 0,
                sniff_frames: Vec::new(),
                press_marks: Vec::new(),
                press_windows: Vec::new(),
                snap_before: HashMap::new(),
                snap_after: HashMap::new(),
                app,
            };
            run_loop(&mut state, rx);
        })
        .expect("spawn session worker");
    WorkerHandle { tx, cancel }
}

fn run_loop(state: &mut WorkerState, rx: Receiver<WorkerCmd>) {
    loop {
        // Non-blocking poll while in streaming modes
        let timeout = match state.mode {
            WorkerMode::Live | WorkerMode::CanSniff => Duration::from_millis(20),
            _ => Duration::from_millis(200),
        };

        match rx.recv_timeout(timeout) {
            Ok(cmd) => handle_cmd(state, cmd),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Err(e) = tick_activity(state) {
                    let _ = state.app.emit("worker.error", e);
                    let _ = state.stop_activity_inner();
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn handle_cmd(state: &mut WorkerState, cmd: WorkerCmd) {
    match cmd {
        WorkerCmd::Connect { port, baud, reply } => {
            let _ = state.stop_activity_inner();
            let result = ScannerSession::connect(&port, baud)
                .and_then(|s| {
                    let status = s.status()?;
                    Ok((s, status))
                })
                .map(|(s, status)| {
                    state.session = Some(s);
                    status
                })
                .map_err(|e| e.to_string());
            let _ = reply.send(result);
        }
        WorkerCmd::ConnectDemo { fixture_id, reply } => {
            let _ = state.stop_activity_inner();
            let result = ScannerSession::connect_demo(&fixture_id)
                .and_then(|s| {
                    let status = s.status()?;
                    Ok((s, status))
                })
                .map(|(s, status)| {
                    state.session = Some(s);
                    status
                })
                .map_err(|e| e.to_string());
            let _ = reply.send(result);
        }
        WorkerCmd::Disconnect { reply } => {
            let _ = state.stop_activity_inner();
            state.session = None;
            let _ = reply.send(Ok(()));
        }
        WorkerCmd::Status { reply } => {
            let result = match &state.session {
                Some(s) => s.status().map(Some).map_err(|e| e.to_string()),
                None => Ok(None),
            };
            let _ = reply.send(result);
        }
        WorkerCmd::SetProfile { profile, reply } => {
            let result = (|| {
                let s = state.session.as_mut().ok_or("not connected")?;
                s.set_profile(profile);
                s.status().map_err(|e| e.to_string())
            })();
            let _ = reply.send(result);
        }
        WorkerCmd::ReadVin { reply } => {
            let result = (|| {
                let s = state.session.as_mut().ok_or("not connected")?;
                s.read_vin().map_err(|e| e.to_string())
            })();
            let _ = reply.send(result);
        }
        WorkerCmd::ScanModules { reply } => {
            let result = (|| {
                ensure_idle(state)?;
                let s = state.session.as_mut().ok_or("not connected")?;
                s.scan_modules().map_err(|e| e.to_string())
            })();
            let _ = reply.send(result);
        }
        WorkerCmd::ReadDtcs { module_id, reply } => {
            let result = (|| {
                ensure_idle(state)?;
                let s = state.session.as_mut().ok_or("not connected")?;
                s.read_module_dtcs(&module_id).map_err(|e| e.to_string())
            })();
            let _ = reply.send(result);
        }
        WorkerCmd::ReadAllDtcs { reply } => {
            let result = (|| {
                ensure_idle(state)?;
                let s = state.session.as_mut().ok_or("not connected")?;
                s.read_all_dtcs().map_err(|e| e.to_string())
            })();
            let _ = reply.send(result);
        }
        WorkerCmd::ClearDtcs { module_id, reply } => {
            let result = (|| {
                ensure_idle(state)?;
                let s = state.session.as_mut().ok_or("not connected")?;
                s.clear_module_dtcs(&module_id).map_err(|e| e.to_string())
            })();
            let _ = reply.send(result);
        }
        WorkerCmd::DumpAsBuilt { module_id, reply } => {
            let result = (|| {
                ensure_idle(state)?;
                let s = state.session.as_mut().ok_or("not connected")?;
                s.dump_as_built(&module_id).map_err(|e| e.to_string())
            })();
            let _ = reply.send(result);
        }
        WorkerCmd::DiscoverEcus {
            bus,
            from_id,
            to_id,
            reply,
        } => {
            let result = (|| {
                ensure_idle(state)?;
                state.cancel.store(false, Ordering::Relaxed);
                state.mode = WorkerMode::EcuDiscover;
                let cancel = state.cancel.clone();
                let app = state.app.clone();
                let s = state.session.as_mut().ok_or("not connected")?;
                let found = s
                    .discover_ecus(bus, from_id, to_id, &cancel, |p: EcuDiscoverProgress| {
                        let _ = app.emit("ecu.progress", &p);
                    })
                    .map_err(|e| e.to_string())?;
                state.mode = WorkerMode::Idle;
                let _ = state.app.emit("ecu.done", &found);
                Ok(found)
            })();
            if result.is_err() {
                state.mode = WorkerMode::Idle;
            }
            let _ = reply.send(result);
        }
        WorkerCmd::StartLive { config, reply } => {
            let result = (|| {
                ensure_idle(state)?;
                let _ = state.session.as_ref().ok_or("not connected")?;
                state.cancel.store(false, Ordering::Relaxed);
                state.live_samples.clear();
                state.live_started_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                state.live_config = Some(config);
                state.mode = WorkerMode::Live;
                let _ = state.app.emit("worker.mode", "live");
                Ok(())
            })();
            let _ = reply.send(result);
        }
        WorkerCmd::ListSupportedPids { reply } => {
            let result = (|| {
                ensure_idle(state)?;
                let s = state.session.as_mut().ok_or("not connected")?;
                s.list_supported_obd_pids().map_err(|e| e.to_string())
            })();
            let _ = reply.send(result);
        }
        WorkerCmd::StartSniff { bus, reply } => {
            let result = (|| {
                ensure_idle(state)?;
                let s = state.session.as_mut().ok_or("not connected")?;
                s.start_can_sniff(bus).map_err(|e| e.to_string())?;
                state.cancel.store(false, Ordering::Relaxed);
                state.sniff_frames.clear();
                state.press_marks.clear();
                state.press_windows.clear();
                state.mode = WorkerMode::CanSniff;
                let _ = state.app.emit("worker.mode", "can_sniff");
                Ok(())
            })();
            let _ = reply.send(result);
        }
        WorkerCmd::MarkPress { reply } => {
            let result = (|| {
                if state.mode != WorkerMode::CanSniff {
                    return Err("not sniffing".into());
                }
                let now = now_ms();
                if let Some(start) = state.press_marks.last().copied() {
                    if now.saturating_sub(start) < 2000 {
                        // close window
                        state.press_windows.push((start, now));
                        state.press_marks.pop();
                        let _ = state.app.emit("sniff.press_window", (start, now));
                        return Ok(());
                    }
                }
                state.press_marks.push(now);
                let _ = state.app.emit("sniff.press_mark", now);
                Ok(())
            })();
            let _ = reply.send(result);
        }
        WorkerCmd::AnalyzePattern { reply } => {
            let result = (|| {
                // Close any open mark as a short window
                if let Some(start) = state.press_marks.pop() {
                    state.press_windows.push((start, now_ms()));
                }
                let c = correlate_press_pattern(&state.sniff_frames, &state.press_windows);
                let _ = state.app.emit("sniff.candidates", &c);
                Ok(c)
            })();
            let _ = reply.send(result);
        }
        WorkerCmd::StartDidSweep {
            module_id,
            from_did,
            to_did,
            reply,
        } => {
            let result = (|| {
                ensure_idle(state)?;
                state.cancel.store(false, Ordering::Relaxed);
                state.mode = WorkerMode::DidSweep;
                let cancel = state.cancel.clone();
                let app = state.app.clone();
                let s = state.session.as_mut().ok_or("not connected")?;
                let found = s
                    .sweep_dids(&module_id, from_did, to_did, &cancel, |v| {
                        let _ = app.emit("did.found", &v);
                    })
                    .map_err(|e| e.to_string())?;
                state.mode = WorkerMode::Idle;
                Ok(found)
            })();
            if result.is_err() {
                state.mode = WorkerMode::Idle;
            }
            let _ = reply.send(result);
        }
        WorkerCmd::DidSnapshot {
            module_id,
            from_did,
            to_did,
            which,
            reply,
        } => {
            let result = (|| {
                ensure_idle(state)?;
                state.cancel.store(false, Ordering::Relaxed);
                let cancel = state.cancel.clone();
                let s = state.session.as_mut().ok_or("not connected")?;
                let snap = s
                    .snapshot_did_range(&module_id, from_did, to_did, &cancel)
                    .map_err(|e| e.to_string())?;
                match which {
                    SnapshotWhich::Before => state.snap_before = snap.clone(),
                    SnapshotWhich::After => state.snap_after = snap.clone(),
                }
                Ok(snap)
            })();
            let _ = reply.send(result);
        }
        WorkerCmd::DiffDidSnapshots { reply } => {
            let diffs = diff_did_snapshots(&state.snap_before, &state.snap_after);
            let _ = reply.send(Ok(diffs));
        }
        WorkerCmd::SaveLiveSession { path, reply } => {
            let result = (|| {
                let port = state
                    .session
                    .as_ref()
                    .map(|s| s.port_name().to_string())
                    .unwrap_or_default();
                let profile_id = state
                    .session
                    .as_ref()
                    .and_then(|s| s.profile().map(|p| p.id.clone()));
                let file = LiveSessionFile {
                    started_at_unix_ms: state.live_started_ms,
                    profile_id,
                    port,
                    samples: state.live_samples.clone(),
                };
                let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
                std::fs::write(&path, json).map_err(|e| e.to_string())?;
                Ok(())
            })();
            let _ = reply.send(result);
        }
        WorkerCmd::GetLiveSamples { reply } => {
            let _ = reply.send(Ok(state.live_samples.clone()));
        }
        WorkerCmd::StopActivity { reply } => {
            let mode = state.stop_activity_inner().unwrap_or(WorkerMode::Idle);
            let _ = reply.send(Ok(mode));
        }
    }
}

fn tick_activity(state: &mut WorkerState) -> Result<(), String> {
    if state.cancel.load(Ordering::Relaxed) {
        let _ = state.stop_activity_inner();
        return Ok(());
    }
    match state.mode {
        WorkerMode::Live => {
            let config = state.live_config.clone().ok_or("no live config")?;
            let s = state.session.as_mut().ok_or("not connected")?;
            let samples = s
                .poll_live_once(&config.pids, &config.dids)
                .map_err(|e| e.to_string())?;
            for sample in &samples {
                let _ = state.app.emit("live.sample", sample);
                // Track unique discoveries by source+id
                let is_new = !state.live_samples.iter().any(|x| {
                    x.source == sample.source && x.id == sample.id
                });
                state.live_samples.push(sample.clone());
                if is_new {
                    match sample.source {
                        LiveSource::Did => {
                            let _ = state.app.emit("live.did_found", sample);
                        }
                        LiveSource::Pid => {
                            let _ = state.app.emit("live.pid_found", sample);
                        }
                    }
                }
            }
            thread::sleep(Duration::from_millis(config.interval_ms.max(50)));
            Ok(())
        }
        WorkerMode::CanSniff => {
            let s = state.session.as_mut().ok_or("not connected")?;
            let frames = s
                .read_sniff_chunk(Duration::from_millis(80))
                .map_err(|e| e.to_string())?;
            for f in frames {
                let _ = state.app.emit("sniff.frame", &f);
                state.sniff_frames.push(f);
                if state.sniff_frames.len() > 50_000 {
                    let drain = state.sniff_frames.len() - 40_000;
                    state.sniff_frames.drain(0..drain);
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn ensure_idle(state: &mut WorkerState) -> Result<(), String> {
    if state.mode != WorkerMode::Idle {
        let _ = state.stop_activity_inner();
    }
    if state.mode != WorkerMode::Idle {
        return Err(format!("worker busy in mode {:?}", state.mode));
    }
    Ok(())
}

impl WorkerState {
    fn stop_activity_inner(&mut self) -> Result<WorkerMode, String> {
        self.cancel.store(true, Ordering::Relaxed);
        let prev = self.mode;
        match prev {
            WorkerMode::CanSniff => {
                if let Some(s) = self.session.as_mut() {
                    let _ = s.stop_can_sniff();
                }
            }
            WorkerMode::Live => {
                self.live_config = None;
            }
            _ => {}
        }
        self.mode = WorkerMode::Idle;
        self.cancel.store(false, Ordering::Relaxed);
        let _ = self.app.emit("worker.mode", "idle");
        Ok(prev)
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
