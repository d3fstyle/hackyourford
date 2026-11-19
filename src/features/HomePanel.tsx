import { ExportMenu } from "../components/ExportMenu";
import type {
  ConnectionMode,
  ConnectionStatus,
  DemoFixtureInfo,
  SerialPortInfo,
  VehicleProfile,
} from "../types";
import { copyText, downloadCsv } from "../utils/export";

interface HomePanelProps {
  connectionMode: ConnectionMode;
  setConnectionMode: (m: ConnectionMode) => void;
  connected: boolean;
  busy: string | null;
  ports: SerialPortInfo[];
  port: string;
  setPort: (p: string) => void;
  fixtures: DemoFixtureInfo[];
  fixtureId: string;
  setFixtureId: (id: string) => void;
  profiles: VehicleProfile[];
  profileId: string;
  setProfileId: (id: string) => void;
  selectedProfile: VehicleProfile | null;
  status: ConnectionStatus | null;
  onRefreshPorts: () => void;
  onConnect: () => void;
  onDisconnect: () => void;
  onScan: () => void;
}

export const HomePanel = ({
  connectionMode,
  setConnectionMode,
  connected,
  busy,
  ports,
  port,
  setPort,
  fixtures,
  fixtureId,
  setFixtureId,
  profiles,
  profileId,
  setProfileId,
  selectedProfile,
  status,
  onRefreshPorts,
  onConnect,
  onDisconnect,
  onScan,
}: HomePanelProps) => (
  <section className="mx-auto flex w-full max-w-3xl flex-col gap-8">
    <div className="space-y-3">
      <p className="text-4xl font-bold leading-tight tracking-tight">FordDiag</p>
      <p className="max-w-xl text-lg text-[var(--text-muted)]">
        Open-source Ford module diagnostics for Linux. Connect your OBDLink MX+,
        pick a vehicle profile, and scan ECUs on HS-CAN and MS-CAN.
      </p>
    </div>

    <div className="flex flex-col gap-2">
      <span className="text-xs uppercase tracking-wider text-[var(--text-muted)]">
        Connection
      </span>
      <div className="flex gap-2">
        <button
          type="button"
          disabled={connected || !!busy}
          onClick={() => setConnectionMode("serial")}
          className={`rounded px-3 py-1.5 text-sm ${
            connectionMode === "serial"
              ? "bg-[var(--accent)] font-semibold text-[var(--bg-deep)]"
              : "border border-[var(--line)] text-[var(--text-muted)] hover:bg-[var(--bg-hover)]"
          } disabled:opacity-40`}
        >
          Serial port
        </button>
        <button
          type="button"
          disabled={connected || !!busy}
          onClick={() => setConnectionMode("demo")}
          className={`rounded px-3 py-1.5 text-sm ${
            connectionMode === "demo"
              ? "bg-[var(--accent)] font-semibold text-[var(--bg-deep)]"
              : "border border-[var(--line)] text-[var(--text-muted)] hover:bg-[var(--bg-hover)]"
          } disabled:opacity-40`}
        >
          Demo
        </button>
      </div>
    </div>

    <div className="grid gap-4 sm:grid-cols-2">
      {connectionMode === "serial" ? (
        <label className="flex flex-col gap-2">
          <span className="text-xs uppercase tracking-wider text-[var(--text-muted)]">
            Serial port
          </span>
          <div className="flex gap-2">
            <select
              className="mono flex-1 rounded border px-3 py-2 text-sm"
              value={port}
              onChange={(e) => setPort(e.target.value)}
              disabled={connected}
            >
              {ports.length === 0 && <option value="">No ports found</option>}
              {ports.map((p) => (
                <option key={p.name} value={p.name}>
                  {p.name} — {p.port_type}
                </option>
              ))}
            </select>
            <button
              type="button"
              onClick={onRefreshPorts}
              className="rounded border border-[var(--line)] px-3 py-2 text-sm text-[var(--text-muted)] hover:bg-[var(--bg-hover)]"
              disabled={!!busy}
            >
              Refresh
            </button>
          </div>
        </label>
      ) : (
        <label className="flex flex-col gap-2">
          <span className="text-xs uppercase tracking-wider text-[var(--text-muted)]">
            Demo fixture
          </span>
          <select
            className="rounded border px-3 py-2 text-sm"
            value={fixtureId}
            onChange={(e) => setFixtureId(e.target.value)}
            disabled={connected}
          >
            {fixtures.length === 0 && <option value="">No fixtures</option>}
            {fixtures.map((f) => (
              <option key={f.id} value={f.id}>
                {f.name}
              </option>
            ))}
          </select>
        </label>
      )}

      <label className="flex flex-col gap-2">
        <span className="text-xs uppercase tracking-wider text-[var(--text-muted)]">
          Vehicle profile
        </span>
        <select
          className="rounded border px-3 py-2 text-sm"
          value={profileId}
          onChange={(e) => setProfileId(e.target.value)}
        >
          {profiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </label>
    </div>

    {selectedProfile?.notes && (
      <p className="text-sm text-[var(--text-muted)]">{selectedProfile.notes}</p>
    )}

    <div className="flex flex-wrap items-center gap-3">
      {!connected ? (
        <button
          type="button"
          onClick={onConnect}
          disabled={!!busy || (connectionMode === "serial" ? !port : !fixtureId)}
          className="rounded bg-[var(--accent)] px-5 py-2.5 text-sm font-semibold text-[var(--bg-deep)] hover:brightness-110 disabled:opacity-40"
        >
          {connectionMode === "demo" ? "Connect demo" : "Connect"}
        </button>
      ) : (
        <>
          <button
            type="button"
            onClick={onScan}
            disabled={!!busy}
            className="rounded bg-[var(--accent)] px-5 py-2.5 text-sm font-semibold text-[var(--bg-deep)] hover:brightness-110 disabled:opacity-40"
          >
            Scan modules
          </button>
          <button
            type="button"
            onClick={onDisconnect}
            disabled={!!busy}
            className="rounded border border-[var(--line)] px-5 py-2.5 text-sm hover:bg-[var(--bg-hover)]"
          >
            Disconnect
          </button>
        </>
      )}
    </div>

    {status && (
      <div className="grid gap-2 border-t border-[var(--line)] pt-6 text-sm">
        <div className="mb-2 flex items-center justify-between">
          <span className="text-xs uppercase tracking-wider text-[var(--text-muted)]">
            Status
          </span>
          <ExportMenu
            onCopy={() =>
              void copyText(
                [
                  `VIN: ${status.vin ?? "—"}`,
                  `Adapter: ${status.adapter.identity}`,
                  `Voltage: ${status.adapter.voltage ?? "—"}`,
                  `MS-CAN: ${status.ms_can_capable}`,
                  `Port: ${status.port}`,
                  `Profile: ${status.profile_id ?? "—"}`,
                ].join("\n"),
              )
            }
            onCsv={() =>
              downloadCsv(`forddiag-home-${Date.now()}.csv`, [
                ["Field", "Value"],
                ["VIN", status.vin ?? ""],
                ["Adapter", status.adapter.identity],
                ["Voltage", String(status.adapter.voltage ?? "")],
                ["MS-CAN", String(status.ms_can_capable)],
                ["Port", status.port],
                ["Profile", status.profile_id ?? ""],
              ])
            }
          />
        </div>
        <div className="flex justify-between">
          <span className="text-[var(--text-muted)]">VIN</span>
          <span className="mono">{status.vin ?? "—"}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-[var(--text-muted)]">MS-CAN</span>
          <span>
            {status.ms_can_capable
              ? "Supported (STN/OBDLink)"
              : "Limited — use MX+ for body modules"}
          </span>
        </div>
      </div>
    )}
  </section>
);
