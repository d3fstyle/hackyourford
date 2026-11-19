//! Ford diagnostic core: STN/ELM transport, ISO-TP, UDS, vehicle profiles.

pub mod as_built;
pub mod as_built_import;
pub mod dtc_catalog;
pub mod error;
pub mod fixture;
pub mod ford_pids;
pub mod isotp;
pub mod live;
pub mod obd;
pub mod obd_dashboard;
pub mod profile;
pub mod scanner;
pub mod security;
pub mod sniff;
pub mod torque;
pub mod transport;
pub mod uds;

pub use as_built::{ParamEdit, ParamValue};
pub use as_built_import::{apply_as_built, parse, summary, ParsedAsBuilt};
pub use dtc_catalog::{format_dtc_display, format_dtc_status, lookup_dtc_info, DtcInfo};
pub use error::{is_link_lost, Error, Result};
pub use fixture::{
    list_builtin_fixtures, load_builtin_fixture, DemoFixture, DemoFixtureInfo, FixtureAdapter,
    FixtureScript,
};
pub use ford_pids::{
    load_catalog as load_ford_extended_pids, load_mode01_scan_supported,
    resolve_ford_extended_pids, resolve_ford_header, FordExtendedPid, ResolvedFordPid,
};
pub use live::{
    DiscoveredEcu, EcuDiscoverProgress, LiveSample, LiveSessionFile, LiveSource, ModuleDtcGroup,
};
pub use obd::{decode_pid, decode_pid_full, pid_label, ObdPidDecoded, ObdPidSample, MODE01_NAMES};
pub use obd_dashboard::{
    DashboardScope, ObdDashboard, ObdDashboardSignalKey, ObdDashboardStore,
};
pub use profile::{
    builtin_profile_by_id, builtin_profiles, list_all_profiles, new_blank_profile,
    ParamDef, ParamKind, ParamOption, ProfileListItem, UserProfileStore, VehicleProfile,
};
pub use scanner::{
    AsBuiltBlockWrite, AsBuiltDump, AsBuiltWriteResult, ConnectionStatus, LiveDidPin,
    ModuleScanResult, ScannerSession, FORD_BARO_DID,
};
pub use security::{algo_by_id, ExperimentalXorSecurity, NoOpSecurity, SecurityKeyAlgo};
pub use sniff::{
    correlate_press_pattern, correlate_press_sequence, diff_did_snapshots, monitor_output_ended,
    CanFrame, CaptureStage, PatternCandidate, parse_monitor_line, parse_monitor_buffer,
};
pub use torque::{evaluate as evaluate_torque_equation, format_value as format_ford_value};
pub use transport::{
    list_serial_ports, AdapterInfo, CanBus, DiagAdapter, MockTransport, SerialPortInfo,
    StnTransport,
};
pub use uds::{DidValue, Dtc};
