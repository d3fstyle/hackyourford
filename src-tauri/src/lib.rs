mod agent_bridge;
mod state;
mod worker;

use ford_core::{
    builtin_profile_by_id, builtin_profiles, list_builtin_fixtures, list_serial_ports, AsBuiltDump,
    CanBus, ConnectionStatus, DemoFixtureInfo, DiscoveredEcu, ModuleDtcGroup, ModuleScanResult,
    PatternCandidate, SerialPortInfo, VehicleProfile, DidValue,
};
use state::AppState;
use std::path::PathBuf;
use tauri::Manager;
use worker::{spawn_worker, LiveConfig, SnapshotWhich};

#[tauri::command]
fn list_ports() -> Result<Vec<SerialPortInfo>, String> {
    list_serial_ports().map_err(|e| e.to_string())
}

#[tauri::command]
fn list_profiles() -> Result<Vec<VehicleProfile>, String> {
    builtin_profiles().map_err(|e| e.to_string())
}

#[tauri::command]
fn list_demo_fixtures() -> Result<Vec<DemoFixtureInfo>, String> {
    list_builtin_fixtures().map_err(|e| e.to_string())
}

#[tauri::command]
fn connect(state: tauri::State<'_, AppState>, port: String, baud: Option<u32>) -> Result<ConnectionStatus, String> {
    let baud = baud.unwrap_or(115_200);
    state.connect(&port, baud)
}

#[tauri::command]
fn connect_demo(state: tauri::State<'_, AppState>, fixture_id: String) -> Result<ConnectionStatus, String> {
    state.connect_demo(&fixture_id)
}

#[tauri::command]
fn disconnect(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.disconnect()
}

#[tauri::command]
fn connection_status(state: tauri::State<'_, AppState>) -> Result<Option<ConnectionStatus>, String> {
    state.status()
}

#[tauri::command]
fn select_profile(state: tauri::State<'_, AppState>, profile_id: String) -> Result<ConnectionStatus, String> {
    let profile = builtin_profile_by_id(&profile_id).map_err(|e| e.to_string())?;
    state.set_profile(profile)
}

#[tauri::command]
fn read_vin(state: tauri::State<'_, AppState>) -> Result<Option<String>, String> {
    state.read_vin()
}

#[tauri::command]
fn scan_modules(state: tauri::State<'_, AppState>) -> Result<Vec<ModuleScanResult>, String> {
    state.scan_modules()
}

#[tauri::command]
fn read_dtcs(state: tauri::State<'_, AppState>, module_id: String) -> Result<Vec<ford_core::Dtc>, String> {
    state.read_dtcs(&module_id)
}

#[tauri::command]
fn read_all_dtcs(state: tauri::State<'_, AppState>) -> Result<Vec<ModuleDtcGroup>, String> {
    state.read_all_dtcs()
}

#[tauri::command]
fn clear_dtcs(state: tauri::State<'_, AppState>, module_id: String) -> Result<(), String> {
    state.clear_dtcs(&module_id)
}

#[tauri::command]
fn dump_as_built(state: tauri::State<'_, AppState>, module_id: String) -> Result<AsBuiltDump, String> {
    state.dump_as_built(&module_id)
}

#[tauri::command]
fn discover_ecus(
    state: tauri::State<'_, AppState>,
    bus: String,
    from_id: u16,
    to_id: u16,
) -> Result<Vec<DiscoveredEcu>, String> {
    let bus = parse_bus(&bus)?;
    state.discover_ecus(bus, from_id, to_id)
}

#[tauri::command]
fn stop_activity(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let mode = state.stop_activity()?;
    Ok(format!("{mode:?}"))
}

#[tauri::command]
fn start_live(
    state: tauri::State<'_, AppState>,
    pids: Vec<u8>,
    dids: Vec<(String, u16, Option<String>)>,
    interval_ms: Option<u64>,
) -> Result<(), String> {
    state.start_live(LiveConfig {
        pids,
        dids,
        interval_ms: interval_ms.unwrap_or(250),
    })
}

#[tauri::command]
fn list_supported_pids(state: tauri::State<'_, AppState>) -> Result<Vec<u8>, String> {
    state.list_supported_pids()
}

#[tauri::command]
fn save_live_session(state: tauri::State<'_, AppState>, path: String) -> Result<(), String> {
    state.save_live_session(PathBuf::from(path))
}

#[tauri::command]
fn get_live_samples(state: tauri::State<'_, AppState>) -> Result<Vec<ford_core::LiveSample>, String> {
    state.get_live_samples()
}

#[tauri::command]
fn start_sniff(state: tauri::State<'_, AppState>, bus: String) -> Result<(), String> {
    state.start_sniff(parse_bus(&bus)?)
}

#[tauri::command]
fn mark_press(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.mark_press()
}

#[tauri::command]
fn analyze_pattern(state: tauri::State<'_, AppState>) -> Result<Vec<PatternCandidate>, String> {
    state.analyze_pattern()
}

#[tauri::command]
fn start_did_sweep(
    state: tauri::State<'_, AppState>,
    module_id: String,
    from_did: u16,
    to_did: u16,
) -> Result<Vec<DidValue>, String> {
    state.start_did_sweep(&module_id, from_did, to_did)
}

#[tauri::command]
fn did_snapshot(
    state: tauri::State<'_, AppState>,
    module_id: String,
    from_did: u16,
    to_did: u16,
    which: String,
) -> Result<HashMapString, String> {
    let which = match which.as_str() {
        "before" => SnapshotWhich::Before,
        "after" => SnapshotWhich::After,
        _ => return Err("which must be before|after".into()),
    };
    let map = state.did_snapshot(&module_id, from_did, to_did, which)?;
    Ok(map
        .into_iter()
        .map(|(k, v)| (format!("{k:04X}"), v))
        .collect())
}

type HashMapString = std::collections::HashMap<String, String>;

#[tauri::command]
fn diff_did_snapshots(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<(u16, String, String)>, String> {
    state.diff_did_snapshots()
}

fn parse_bus(s: &str) -> Result<CanBus, String> {
    match s.to_ascii_lowercase().as_str() {
        "hs" | "hscan" | "hs-can" => Ok(CanBus::Hs),
        "ms" | "mscan" | "ms-can" => Ok(CanBus::Ms),
        _ => Err(format!("unknown bus: {s}")),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = spawn_worker(app.handle().clone());
            agent_bridge::spawn_bridge(app.handle().clone(), handle.clone());
            app.manage(AppState { worker: handle });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_ports,
            list_profiles,
            list_demo_fixtures,
            connect,
            connect_demo,
            disconnect,
            connection_status,
            select_profile,
            read_vin,
            scan_modules,
            read_dtcs,
            read_all_dtcs,
            clear_dtcs,
            dump_as_built,
            discover_ecus,
            stop_activity,
            start_live,
            list_supported_pids,
            save_live_session,
            get_live_samples,
            start_sniff,
            mark_press,
            analyze_pattern,
            start_did_sweep,
            did_snapshot,
            diff_did_snapshots,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FordDiag");
}
