//! Localhost HTTP bridge so Cursor MCP can drive the same session worker as the UI.

use crate::state::AppState;
use crate::worker::{LiveConfig, SnapshotWhich, WorkerHandle};
use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ford_core::{
    builtin_profile_by_id, list_all_profiles, list_builtin_fixtures, list_serial_ports, CanBus,
    LiveDidPin, UserProfileStore,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tower_http::cors::{Any, CorsLayer};

pub const DEFAULT_PORT: u16 = 8787;

#[derive(Clone)]
struct BridgeState {
    api: Arc<AppState>,
    token: String,
    app: AppHandle,
}

#[derive(Serialize, Clone)]
pub struct BridgeInfo {
    pub port: u16,
    pub token_path: String,
}

/// Spawn the agent bridge on a background thread with its own Tokio runtime.
pub fn spawn_bridge(app: AppHandle, worker: WorkerHandle) {
    std::thread::Builder::new()
        .name("forddiag-agent-bridge".into())
        .spawn(move || {
            if let Err(e) = run_bridge(app, worker) {
                eprintln!("[forddiag] agent bridge failed: {e}");
            }
        })
        .expect("spawn agent bridge thread");
}

fn run_bridge(app: AppHandle, worker: WorkerHandle) -> Result<(), String> {
    let token = generate_token();
    let token_path = write_token_file(&token)?;

    let api = Arc::new(AppState { worker });
    let state = BridgeState {
        api,
        token: token.clone(),
        app: app.clone(),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app_router = Router::new()
        .route("/health", get(health))
        .route("/status", get(status))
        .route("/ports", get(ports))
        .route("/profiles", get(profiles))
        .route("/fixtures", get(fixtures))
        .route("/connect", post(connect))
        .route("/connect_demo", post(connect_demo))
        .route("/disconnect", post(disconnect))
        .route("/profile", post(select_profile))
        .route("/read_vin", post(read_vin))
        .route("/scan_modules", post(scan_modules))
        .route("/discover_ecus", post(discover_ecus))
        .route("/read_all_dtcs", post(read_all_dtcs))
        .route("/read_dtcs", post(read_dtcs))
        .route("/clear_dtcs", post(clear_dtcs))
        .route("/dump_as_built", post(dump_as_built))
        .route("/write_as_built", post(write_as_built))
        .route("/decode_as_built_params", post(decode_as_built_params))
        .route("/write_as_built_params", post(write_as_built_params))
        .route("/live/start", post(live_start))
        .route("/live/stop", post(live_stop))
        .route("/live/samples", get(live_samples))
        .route("/live/save", post(live_save))
        .route("/live/supported_pids", get(supported_pids))
        .route("/sniff/start", post(sniff_start))
        .route("/sniff/mark_press", post(sniff_mark_press))
        .route("/sniff/analyze", post(sniff_analyze))
        .route("/did/sweep", post(did_sweep))
        .route("/did/snapshot", post(did_snapshot))
        .route("/did/diff", post(did_diff))
        .route("/stop", post(stop_activity))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT));
    let info = BridgeInfo {
        port: DEFAULT_PORT,
        token_path: token_path.display().to_string(),
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .map_err(|e| e.to_string())?;

    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| format!("bind {addr}: {e}"))?;
        let _ = app.emit("agent.bridge", &info);
        eprintln!(
            "[forddiag] agent bridge listening on http://{addr} (token: {})",
            info.token_path
        );
        axum::serve(listener, app_router)
            .await
            .map_err(|e| e.to_string())
    })
}

fn generate_token() -> String {
    let mut bytes = [0u8; 24];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    hex::encode(bytes)
}

fn write_token_file(token: &str) -> Result<PathBuf, String> {
    let path = if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        let dir = PathBuf::from(runtime).join("forddiag");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        dir.join("agent.token")
    } else {
        PathBuf::from("/tmp/forddiag-agent.token")
    };
    std::fs::write(&path, token).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(path)
}

async fn auth_middleware(
    State(state): State<BridgeState>,
    req: Request,
    next: Next,
) -> Response {
    if req.uri().path() == "/health" {
        return next.run(req).await;
    }
    let authorized = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            v.strip_prefix("Bearer ")
                .map(|t| t == state.token)
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "missing or invalid Bearer token" })),
        )
            .into_response();
    }
    next.run(req).await
}

fn api_err(e: String) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": e })))
}

fn parse_bus(s: &str) -> Result<CanBus, String> {
    match s.to_ascii_lowercase().as_str() {
        "hs" | "hscan" | "hs-can" => Ok(CanBus::Hs),
        "ms" | "mscan" | "ms-can" => Ok(CanBus::Ms),
        _ => Err(format!("unknown bus: {s}")),
    }
}

async fn health() -> Json<Value> {
    Json(json!({
        "ok": true,
        "version": env!("CARGO_PKG_VERSION"),
        "service": "forddiag-agent-bridge",
    }))
}

async fn status(State(s): State<BridgeState>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let st = s.api.status().map_err(api_err)?;
    Ok(Json(serde_json::to_value(st).unwrap_or(Value::Null)))
}

async fn ports() -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let list = list_serial_ports().map_err(|e| api_err(e.to_string()))?;
    Ok(Json(serde_json::to_value(list).unwrap_or(Value::Null)))
}

async fn profiles(State(s): State<BridgeState>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let profiles_dir = s
        .app
        .path()
        .app_data_dir()
        .map(|p| p.join("profiles"))
        .ok();
    let list = list_all_profiles(profiles_dir).map_err(|e| api_err(e.to_string()))?;
    Ok(Json(serde_json::to_value(list).unwrap_or(Value::Null)))
}

async fn fixtures() -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let list = list_builtin_fixtures().map_err(|e| api_err(e.to_string()))?;
    Ok(Json(serde_json::to_value(list).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
struct ConnectBody {
    port: String,
    baud: Option<u32>,
}

async fn connect(
    State(s): State<BridgeState>,
    Json(body): Json<ConnectBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let st = s
        .api
        .connect(&body.port, body.baud.unwrap_or(115_200))
        .map_err(api_err)?;
    Ok(Json(serde_json::to_value(st).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
struct ConnectDemoBody {
    fixture_id: String,
}

async fn connect_demo(
    State(s): State<BridgeState>,
    Json(body): Json<ConnectDemoBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let st = s.api.connect_demo(&body.fixture_id).map_err(api_err)?;
    Ok(Json(serde_json::to_value(st).unwrap_or(Value::Null)))
}

async fn disconnect(
    State(s): State<BridgeState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    s.api.disconnect().map_err(api_err)?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct ProfileBody {
    profile_id: String,
}

async fn select_profile(
    State(s): State<BridgeState>,
    Json(body): Json<ProfileBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let profiles_dir = s
        .app
        .path()
        .app_data_dir()
        .map(|p| p.join("profiles"));
    if let Ok(dir) = profiles_dir {
        if let Ok(store) = UserProfileStore::new(dir) {
            if let Ok(user_profiles) = store.load_user_profiles() {
                if let Some(profile) = user_profiles.into_iter().find(|p| p.id == body.profile_id) {
                    let st = s.api.set_profile(profile).map_err(api_err)?;
                    return Ok(Json(serde_json::to_value(st).unwrap_or(Value::Null)));
                }
            }
        }
    }
    let profile = builtin_profile_by_id(&body.profile_id).map_err(|e| api_err(e.to_string()))?;
    let st = s.api.set_profile(profile).map_err(api_err)?;
    Ok(Json(serde_json::to_value(st).unwrap_or(Value::Null)))
}

async fn read_vin(State(s): State<BridgeState>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let vin = s.api.read_vin().map_err(api_err)?;
    Ok(Json(json!({ "vin": vin })))
}

async fn scan_modules(
    State(s): State<BridgeState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let r = s.api.scan_modules().map_err(api_err)?;
    Ok(Json(serde_json::to_value(r).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
struct DiscoverBody {
    bus: String,
    from_id: u16,
    to_id: u16,
}

async fn discover_ecus(
    State(s): State<BridgeState>,
    Json(body): Json<DiscoverBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let bus = parse_bus(&body.bus).map_err(api_err)?;
    let r = s
        .api
        .discover_ecus(bus, body.from_id, body.to_id)
        .map_err(api_err)?;
    Ok(Json(serde_json::to_value(r).unwrap_or(Value::Null)))
}

async fn read_all_dtcs(
    State(s): State<BridgeState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let r = s.api.read_all_dtcs().map_err(api_err)?;
    Ok(Json(serde_json::to_value(r).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
struct ModuleBody {
    module_id: String,
}

async fn read_dtcs(
    State(s): State<BridgeState>,
    Json(body): Json<ModuleBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let r = s.api.read_dtcs(&body.module_id).map_err(api_err)?;
    Ok(Json(serde_json::to_value(r).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
struct ClearDtcBody {
    module_id: String,
    confirm: Option<bool>,
}

async fn clear_dtcs(
    State(s): State<BridgeState>,
    Json(body): Json<ClearDtcBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if body.confirm != Some(true) {
        return Err(api_err(
            "clear_dtcs requires confirm: true (destructive)".into(),
        ));
    }
    s.api.clear_dtcs(&body.module_id).map_err(api_err)?;
    Ok(Json(json!({ "ok": true })))
}

async fn dump_as_built(
    State(s): State<BridgeState>,
    Json(body): Json<ModuleBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let r = s.api.dump_as_built(&body.module_id).map_err(api_err)?;
    Ok(Json(serde_json::to_value(r).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
struct WriteAsBuiltBlockBody {
    did: u16,
    /// Hex string (with or without spaces) or raw byte array via serde.
    #[serde(alias = "hex")]
    data_hex: Option<String>,
    data: Option<Vec<u8>>,
}

#[derive(Deserialize)]
struct WriteAsBuiltBody {
    module_id: String,
    blocks: Vec<WriteAsBuiltBlockBody>,
    fix_checksums: Option<bool>,
    verify: Option<bool>,
    /// Must be true — writing As-Built can brick vehicle configuration.
    confirm: Option<bool>,
}

async fn write_as_built(
    State(s): State<BridgeState>,
    Json(body): Json<WriteAsBuiltBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if body.confirm != Some(true) {
        return Err(api_err(
            "write_as_built requires confirm: true (writes ECU configuration)".into(),
        ));
    }
    let mut blocks = Vec::new();
    for b in body.blocks {
        let data = if let Some(bytes) = b.data {
            bytes
        } else if let Some(hex_str) = b.data_hex {
            let cleaned: String = hex_str.chars().filter(|c| !c.is_whitespace()).collect();
            hex::decode(&cleaned).map_err(|e| api_err(format!("invalid hex for DID {:04X}: {e}", b.did)))?
        } else {
            return Err(api_err(format!(
                "DID {:04X}: provide data (bytes) or data_hex/hex",
                b.did
            )));
        };
        blocks.push(ford_core::AsBuiltBlockWrite { did: b.did, data });
    }
    let r = s
        .api
        .write_as_built(
            &body.module_id,
            blocks,
            body.fix_checksums.unwrap_or(false),
            body.verify.unwrap_or(true),
        )
        .map_err(api_err)?;
    Ok(Json(serde_json::to_value(r).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
struct DecodeParamsBody {
    module_id: String,
}

async fn decode_as_built_params(
    State(s): State<BridgeState>,
    Json(body): Json<DecodeParamsBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let r = s
        .api
        .decode_as_built_params(&body.module_id)
        .map_err(api_err)?;
    Ok(Json(serde_json::to_value(r).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
struct ParamEditBody {
    id: String,
    value: u64,
}

#[derive(Deserialize)]
struct WriteAsBuiltParamsBody {
    module_id: String,
    edits: Vec<ParamEditBody>,
    fix_checksums: Option<bool>,
    verify: Option<bool>,
    confirm: Option<bool>,
}

async fn write_as_built_params(
    State(s): State<BridgeState>,
    Json(body): Json<WriteAsBuiltParamsBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if body.confirm != Some(true) {
        return Err(api_err(
            "write_as_built_params requires confirm: true (writes ECU configuration)".into(),
        ));
    }
    let edits = body
        .edits
        .into_iter()
        .map(|e| ford_core::ParamEdit {
            id: e.id,
            value: e.value,
        })
        .collect();
    let r = s
        .api
        .write_as_built_params(
            &body.module_id,
            edits,
            body.fix_checksums.unwrap_or(false),
            body.verify.unwrap_or(true),
        )
        .map_err(api_err)?;
    Ok(Json(serde_json::to_value(r).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
struct LiveStartBody {
    pids: Vec<u8>,
    #[serde(default)]
    dids: Vec<serde_json::Value>,
    interval_ms: Option<u64>,
}

async fn live_start(
    State(s): State<BridgeState>,
    Json(body): Json<LiveStartBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Parse LiveDidPin from JSON (supports both tuple and object forms)
    let did_pins: Vec<LiveDidPin> = body.dids
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
    
    s.api
        .start_live(LiveConfig {
            pids: body.pids,
            dids: did_pins,
            interval_ms: body.interval_ms.unwrap_or(250),
        })
        .map_err(api_err)?;
    Ok(Json(json!({ "ok": true })))
}

async fn live_stop(State(s): State<BridgeState>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mode = s.api.stop_activity().map_err(api_err)?;
    Ok(Json(json!({ "ok": true, "previous_mode": format!("{mode:?}") })))
}

async fn live_samples(
    State(s): State<BridgeState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let r = s.api.get_live_samples().map_err(api_err)?;
    Ok(Json(serde_json::to_value(r).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
struct SaveBody {
    path: String,
}

async fn live_save(
    State(s): State<BridgeState>,
    Json(body): Json<SaveBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    s.api
        .save_live_session(PathBuf::from(body.path))
        .map_err(api_err)?;
    Ok(Json(json!({ "ok": true })))
}

async fn supported_pids(
    State(s): State<BridgeState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let r = s.api.list_supported_pids().map_err(api_err)?;
    Ok(Json(json!({ "pids": r })))
}

#[derive(Deserialize)]
struct BusBody {
    bus: String,
}

async fn sniff_start(
    State(s): State<BridgeState>,
    Json(body): Json<BusBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let bus = parse_bus(&body.bus).map_err(api_err)?;
    s.api.start_sniff(bus).map_err(api_err)?;
    Ok(Json(json!({ "ok": true })))
}

async fn sniff_mark_press(
    State(s): State<BridgeState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    s.api.mark_press().map_err(api_err)?;
    Ok(Json(json!({ "ok": true })))
}

async fn sniff_analyze(
    State(s): State<BridgeState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let r = s.api.analyze_pattern().map_err(api_err)?;
    Ok(Json(serde_json::to_value(r).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
struct DidSweepBody {
    module_id: String,
    from_did: u16,
    to_did: u16,
}

async fn did_sweep(
    State(s): State<BridgeState>,
    Json(body): Json<DidSweepBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let r = s
        .api
        .start_did_sweep(&body.module_id, body.from_did, body.to_did)
        .map_err(api_err)?;
    Ok(Json(serde_json::to_value(r).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
struct DidSnapshotBody {
    module_id: String,
    from_did: u16,
    to_did: u16,
    which: String,
}

async fn did_snapshot(
    State(s): State<BridgeState>,
    Json(body): Json<DidSnapshotBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let which = match body.which.as_str() {
        "before" => SnapshotWhich::Before,
        "after" => SnapshotWhich::After,
        _ => return Err(api_err("which must be before|after".into())),
    };
    let map = s
        .api
        .did_snapshot(&body.module_id, body.from_did, body.to_did, which)
        .map_err(api_err)?;
    let obj: serde_json::Map<String, Value> = map
        .into_iter()
        .map(|(k, v)| (format!("{k:04X}"), Value::String(v)))
        .collect();
    Ok(Json(Value::Object(obj)))
}

async fn did_diff(State(s): State<BridgeState>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let r = s.api.diff_did_snapshots().map_err(api_err)?;
    Ok(Json(serde_json::to_value(r).unwrap_or(Value::Null)))
}

async fn stop_activity(
    State(s): State<BridgeState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mode = s.api.stop_activity().map_err(api_err)?;
    Ok(Json(json!({ "ok": true, "previous_mode": format!("{mode:?}") })))
}
