mod agent_bridge;
mod state;
mod worker;

use ford_core::{
    apply_as_built, builtin_profile_by_id, list_all_profiles, list_builtin_fixtures,
    list_serial_ports, load_ford_extended_pids, load_mode01_scan_supported, lookup_dtc_info,
    new_blank_profile, parse as parse_as_built, resolve_ford_extended_pids,
    summary as as_built_summary, AsBuiltBlockWrite, AsBuiltDump, AsBuiltWriteResult, CanBus,
    CaptureStage, ConnectionStatus, DashboardScope, DemoFixtureInfo, DidValue, DiscoveredEcu,
    DtcInfo, LiveDidPin, ModuleDtcGroup, ModuleScanResult, ObdDashboard, ObdDashboardStore,
    ParamEdit, ParamValue, ParsedAsBuilt, PatternCandidate, ProfileListItem, SerialPortInfo,
    UserProfileStore, VehicleProfile, MODE01_NAMES,
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
fn list_profiles(app: tauri::AppHandle) -> Result<Vec<ProfileListItem>, String> {
    let profiles_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles");
    list_all_profiles(Some(profiles_dir)).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_profile(
    app: tauri::AppHandle,
    profile: VehicleProfile,
) -> Result<(), String> {
    let profiles_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles");
    let store = UserProfileStore::new(profiles_dir).map_err(|e| e.to_string())?;
    store.save(&profile).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_profile(app: tauri::AppHandle, profile_id: String) -> Result<bool, String> {
    let profiles_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles");
    let store = UserProfileStore::new(profiles_dir).map_err(|e| e.to_string())?;
    store.delete(&profile_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn can_delete_profile(app: tauri::AppHandle, profile_id: String) -> Result<bool, String> {
    let profiles_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles");
    let store = UserProfileStore::new(profiles_dir).map_err(|e| e.to_string())?;
    Ok(store.can_delete(&profile_id))
}

#[tauri::command]
fn create_blank_profile(id: String, name: String, years: String) -> Result<VehicleProfile, String> {
    Ok(new_blank_profile(id, name, years))
}

#[tauri::command]
fn import_as_built_xml(xml_content: String) -> Result<ParsedAsBuilt, String> {
    parse_as_built(&xml_content).map_err(|e| e.to_string())
}

#[tauri::command]
fn apply_as_built_to_profile(
    profile: VehicleProfile,
    parsed: ParsedAsBuilt,
) -> Result<VehicleProfile, String> {
    Ok(apply_as_built(&profile, &parsed))
}

#[tauri::command]
fn as_built_import_summary(parsed: ParsedAsBuilt) -> Result<String, String> {
    Ok(as_built_summary(&parsed))
}

#[tauri::command]
fn read_text_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
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
fn select_profile(app: tauri::AppHandle, state: tauri::State<'_, AppState>, profile_id: String) -> Result<ConnectionStatus, String> {
    let profiles_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles");
    
    // Try user profile first
    if let Ok(store) = UserProfileStore::new(profiles_dir.clone()) {
        if let Ok(user_profiles) = store.load_user_profiles() {
            if let Some(profile) = user_profiles.into_iter().find(|p| p.id == profile_id) {
                return state.set_profile(profile);
            }
        }
    }
    
    // Fall back to builtin
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
fn write_as_built(
    state: tauri::State<'_, AppState>,
    module_id: String,
    blocks: Vec<AsBuiltBlockWrite>,
    fix_checksums: Option<bool>,
    verify: Option<bool>,
) -> Result<AsBuiltWriteResult, String> {
    state.write_as_built(
        &module_id,
        blocks,
        fix_checksums.unwrap_or(false),
        verify.unwrap_or(true),
    )
}

#[tauri::command]
fn decode_as_built_params(
    state: tauri::State<'_, AppState>,
    module_id: String,
) -> Result<Vec<ParamValue>, String> {
    state.decode_as_built_params(&module_id)
}

#[tauri::command]
fn write_as_built_params(
    state: tauri::State<'_, AppState>,
    module_id: String,
    edits: Vec<ParamEdit>,
    fix_checksums: Option<bool>,
    verify: Option<bool>,
) -> Result<AsBuiltWriteResult, String> {
    state.write_as_built_params(
        &module_id,
        edits,
        fix_checksums.unwrap_or(false),
        verify.unwrap_or(true),
    )
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
    dids: Vec<serde_json::Value>,
    interval_ms: Option<u64>,
) -> Result<(), String> {
    // Parse LiveDidPin from JSON (supports both tuple and object forms)
    let did_pins: Vec<LiveDidPin> = dids
        .into_iter()
        .filter_map(|v| {
            // Try object form first
            if let Ok(pin) = serde_json::from_value::<LiveDidPin>(v.clone()) {
                return Some(pin);
            }
            // Try tuple form (module_id, did, label)
            if let Ok((module_id, did, label)) = serde_json::from_value::<(String, u16, Option<String>)>(v) {
                return Some(LiveDidPin {
                    module_id,
                    did,
                    label,
                    equation: None,
                    unit: None,
                });
            }
            None
        })
        .collect();
    
    state.start_live(LiveConfig {
        pids,
        dids: did_pins,
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
fn analyze_press_sequence(
    state: tauri::State<'_, AppState>,
    stages: Vec<CaptureStage>,
) -> Result<Vec<PatternCandidate>, String> {
    state.analyze_press_sequence(stages)
}

#[tauri::command]
fn list_mode01_pid_meta() -> Result<Vec<serde_json::Value>, String> {
    let mut list = Vec::new();
    for (pid, label) in MODE01_NAMES.iter().enumerate() {
        if !label.is_empty() {
            list.push(serde_json::json!({
                "pid": pid as u8,
                "label": label,
            }));
        }
    }
    Ok(list)
}

#[tauri::command]
fn list_ford_extended_pids(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    profile_id: Option<String>,
) -> Result<Vec<serde_json::Value>, String> {
    let catalog = load_ford_extended_pids().map_err(|e| e.to_string())?;

    let profile = if let Some(pid) = profile_id {
        let profiles_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("profiles");
        if let Ok(store) = UserProfileStore::new(profiles_dir) {
            if let Ok(user_profiles) = store.load_user_profiles() {
                if let Some(p) = user_profiles.into_iter().find(|p| p.id == pid) {
                    Some(p)
                } else {
                    builtin_profile_by_id(&pid).ok()
                }
            } else {
                builtin_profile_by_id(&pid).ok()
            }
        } else {
            builtin_profile_by_id(&pid).ok()
        }
    } else {
        state.get_profile()
    };
    
    if let Some(prof) = profile {
        let resolved = resolve_ford_extended_pids(&catalog, &prof);
        Ok(resolved
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "name": r.pid.name,
                    "short_name": r.pid.short_name,
                    "mode_and_pid": r.pid.mode_and_pid,
                    "equation": r.pid.equation,
                    "unit": r.pid.unit,
                    "module_id": r.module_id,
                    "min": r.pid.min,
                    "max": r.pid.max,
                })
            })
            .collect())
    } else {
        Ok(Vec::new())
    }
}

#[tauri::command]
fn lookup_dtc(code: String) -> Result<Option<DtcInfo>, String> {
    Ok(lookup_dtc_info(&code))
}

#[tauri::command]
fn list_mode01_scan_supported() -> Result<Vec<u8>, String> {
    let set = load_mode01_scan_supported().map_err(|e| e.to_string())?;
    let mut list: Vec<u8> = set.into_iter().collect();
    list.sort();
    Ok(list)
}

#[tauri::command]
fn list_obd_dashboards(
    app: tauri::AppHandle,
    scope: String,
    profile_id: Option<String>,
) -> Result<Vec<ObdDashboard>, String> {
    let dashboards_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("obd_dashboards");
    let store = ObdDashboardStore::new(dashboards_dir).map_err(|e| e.to_string())?;
    
    let scope_enum = match scope.as_str() {
        "global" => DashboardScope::Global,
        "profile" => DashboardScope::Profile,
        _ => return Err("scope must be global or profile".into()),
    };
    
    store
        .list(scope_enum, profile_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn save_obd_dashboard(
    app: tauri::AppHandle,
    dashboard: ObdDashboard,
) -> Result<(), String> {
    let dashboards_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("obd_dashboards");
    let store = ObdDashboardStore::new(dashboards_dir).map_err(|e| e.to_string())?;
    store.save(&dashboard).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_obd_dashboard(
    app: tauri::AppHandle,
    scope: String,
    profile_id: Option<String>,
    id: String,
) -> Result<bool, String> {
    let dashboards_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("obd_dashboards");
    let store = ObdDashboardStore::new(dashboards_dir).map_err(|e| e.to_string())?;
    
    let scope_enum = match scope.as_str() {
        "global" => DashboardScope::Global,
        "profile" => DashboardScope::Profile,
        _ => return Err("scope must be global or profile".into()),
    };
    
    store
        .delete(scope_enum, profile_id.as_deref(), &id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn copy_obd_dashboard_to_profile(
    app: tauri::AppHandle,
    dashboard: ObdDashboard,
    target_profile_id: String,
) -> Result<ObdDashboard, String> {
    let dashboards_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("obd_dashboards");
    let store = ObdDashboardStore::new(dashboards_dir).map_err(|e| e.to_string())?;
    store
        .copy_to_profile(&dashboard, &target_profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_obd_favorite(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let dashboards_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("obd_dashboards");
    let store = ObdDashboardStore::new(dashboards_dir).map_err(|e| e.to_string())?;
    store.get_favorite().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_obd_favorite(
    app: tauri::AppHandle,
    dashboard_id: Option<String>,
) -> Result<(), String> {
    let dashboards_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("obd_dashboards");
    let store = ObdDashboardStore::new(dashboards_dir).map_err(|e| e.to_string())?;
    store.set_favorite(dashboard_id).map_err(|e| e.to_string())
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
            save_profile,
            delete_profile,
            can_delete_profile,
            create_blank_profile,
            import_as_built_xml,
            apply_as_built_to_profile,
            as_built_import_summary,
            read_text_file,
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
            write_as_built,
            decode_as_built_params,
            write_as_built_params,
            discover_ecus,
            stop_activity,
            start_live,
            list_supported_pids,
            save_live_session,
            get_live_samples,
            start_sniff,
            mark_press,
            analyze_pattern,
            analyze_press_sequence,
            start_did_sweep,
            did_snapshot,
            diff_did_snapshots,
            list_mode01_pid_meta,
            list_ford_extended_pids,
            lookup_dtc,
            list_mode01_scan_supported,
            list_obd_dashboards,
            save_obd_dashboard,
            delete_obd_dashboard,
            copy_obd_dashboard_to_profile,
            get_obd_favorite,
            set_obd_favorite,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FordDiag");
}
