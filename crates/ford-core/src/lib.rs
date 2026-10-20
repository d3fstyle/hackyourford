//! Ford diagnostic core: STN/ELM transport, ISO-TP, UDS, vehicle profiles.

pub mod error;
pub mod fixture;
pub mod isotp;
pub mod live;
pub mod obd;
pub mod profile;
pub mod scanner;
pub mod sniff;
pub mod transport;
pub mod uds;

pub use error::{Error, Result};
pub use fixture::{
    list_builtin_fixtures, load_builtin_fixture, DemoFixture, DemoFixtureInfo, FixtureAdapter,
    FixtureScript,
};
pub use live::{
    DiscoveredEcu, EcuDiscoverProgress, LiveSample, LiveSessionFile, LiveSource, ModuleDtcGroup,
};
pub use profile::{builtin_profile_by_id, builtin_profiles, VehicleProfile};
pub use scanner::{AsBuiltDump, ConnectionStatus, ModuleScanResult, ScannerSession};
pub use sniff::{correlate_press_pattern, diff_did_snapshots, CanFrame, PatternCandidate};
pub use transport::{
    list_serial_ports, AdapterInfo, CanBus, DiagAdapter, MockTransport, SerialPortInfo, StnTransport,
};
pub use uds::{DidValue, Dtc};
