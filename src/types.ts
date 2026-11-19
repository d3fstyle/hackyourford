export interface SerialPortInfo {
  name: string;
  port_type: string;
}

export interface DemoFixtureInfo {
  id: string;
  name: string;
  profile_hint: string;
}

export type ConnectionMode = "serial" | "demo";

export interface AdapterInfo {
  identity: string;
  is_stn: boolean;
  voltage: number | null;
}

export interface ConnectionStatus {
  port: string;
  adapter: AdapterInfo;
  profile_id: string | null;
  vin: string | null;
  ms_can_capable: boolean;
}

export interface DidDef {
  did: number;
  label: string;
  as_built?: boolean;
}

export type ParamKind = "bool" | "uint" | "enum";

export interface ParamOption {
  value: number;
  label: string;
}

export interface ParamDef {
  id: string;
  label: string;
  did: number;
  byte: number;
  bit: number;
  width: number;
  kind: ParamKind;
  options?: ParamOption[];
  verified?: boolean;
  notes?: string | null;
}

export interface ParamValue {
  id: string;
  label: string;
  did: number;
  kind: ParamKind;
  value: number;
  verified: boolean;
  notes: string | null;
  options: ParamOption[];
}

export interface ParamEdit {
  id: string;
  value: number;
}

export interface ModuleDef {
  id: string;
  name: string;
  tx_id: number;
  rx_id?: number | null;
  bus: "hs" | "ms";
  dids: DidDef[];
  params?: ParamDef[];
}

export interface VehicleProfile {
  id: string;
  name: string;
  years: string;
  notes?: string | null;
  modules: ModuleDef[];
}

export interface DidValue {
  did: number;
  data: number[];
  hex: string;
  ascii: string | null;
}

export interface ModuleScanResult {
  id: string;
  name: string;
  tx_id: number;
  bus: "hs" | "ms";
  online: boolean;
  error: string | null;
  identity: DidValue[];
}

export interface Dtc {
  code: string;
  status: number;
  raw: number;
  test_failed: boolean;
  pending: boolean;
  confirmed: boolean;
}

export interface ModuleDtcGroup {
  module_id: string;
  module_name: string;
  tx_id: number;
  bus: "hs" | "ms";
  online: boolean;
  error: string | null;
  dtcs: Dtc[];
}

export interface AsBuiltDump {
  module_id: string;
  module_name: string;
  profile_id: string;
  blocks: DidValue[];
  exported_at_unix_ms: number;
}

export interface AsBuiltBlockWrite {
  did: number;
  data: number[];
}

export interface AsBuiltWriteResult {
  module_id: string;
  profile_id: string;
  written_dids: number[];
  verified: DidValue[] | null;
  checksum_fixed: boolean;
}

export interface DiscoveredEcu {
  tx_id: number;
  rx_id: number;
  bus: "hs" | "ms";
  online: boolean;
  profile_id: string | null;
  name: string | null;
  identity: DidValue[];
  error: string | null;
}

export interface EcuDiscoverProgress {
  bus: "hs" | "ms";
  current_id: number;
  from_id: number;
  to_id: number;
  found: number;
}

export type LiveSource = "did" | "pid";

export interface LiveSample {
  ts_ms: number;
  source: LiveSource;
  id: number;
  raw_hex: string;
  decoded: string | null;
  module_id: string | null;
  label: string | null;
  value?: number;
  unit?: string;
}

export interface CanFrame {
  ts_ms: number;
  id: number;
  data: number[];
  raw_line: string;
}

export interface PatternCandidate {
  can_id: number;
  byte_index: number;
  bit_index: number | null;
  change_score: number;
  sample_values: string[];
}

export type AppTab = "home" | "profiles" | "ecus" | "dtcs" | "live" | "obd";

export interface ProfileListItem {
  profile: VehicleProfile;
  is_builtin: boolean;
  can_delete: boolean;
}

export interface LiveDidPin {
  module_id: string;
  did: number;
  label?: string;
  equation?: string;
  unit?: string;
}

export interface DtcInfo {
  name: string;
  description: string;
}

export interface ParsedAsBuilt {
  vin: string | null;
  node_ids: number[];
  blocks_by_addr: Record<string, number[]>;
}

export interface ObdDashboardSignalKey {
  source: string;
  id: string;
}

export interface ObdDashboard {
  id: string;
  name: string;
  scope: "global" | "profile";
  profile_id: string | null;
  signals: ObdDashboardSignalKey[];
  created_at_unix_ms?: number;
  updated_at_unix_ms?: number;
}

export interface CaptureStage {
  expected_presses: number;
  start_ms: number;
  end_ms: number;
}

export interface FordExtendedPidRow {
  name: string;
  short_name: string;
  mode_and_pid: string;
  equation: string;
  unit: string;
  module_id: string;
  min?: number;
  max?: number;
}

export function hexId(n: number, width = 3): string {
  return `0x${n.toString(16).toUpperCase().padStart(width, "0")}`;
}

