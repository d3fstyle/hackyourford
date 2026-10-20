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

export interface ModuleDef {
  id: string;
  name: string;
  tx_id: number;
  rx_id?: number | null;
  bus: "hs" | "ms";
  dids: DidDef[];
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

export type AppTab = "home" | "ecus" | "dtcs" | "live" | "discover";

export function hexId(n: number, width = 3): string {
  return `0x${n.toString(16).toUpperCase().padStart(width, "0")}`;
}

export function downloadJson(filename: string, data: unknown) {
  const blob = new Blob([JSON.stringify(data, null, 2)], {
    type: "application/json",
  });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}
