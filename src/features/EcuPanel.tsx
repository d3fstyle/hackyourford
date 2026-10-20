import { useMemo, useState } from "react";
import type {
  AsBuiltDump,
  ConnectionStatus,
  DiscoveredEcu,
  EcuDiscoverProgress,
  ModuleScanResult,
} from "../types";
import { downloadJson, hexId } from "../types";

interface EcuPanelProps {
  status: ConnectionStatus | null;
  modules: ModuleScanResult[];
  discovered: DiscoveredEcu[];
  discoverProgress: EcuDiscoverProgress | null;
  busy: string | null;
  onScanProfile: () => void;
  onDiscover: (bus: "hs" | "ms", fromId: number, toId: number) => void;
  onStop: () => void;
  onDumpAsBuilt: (moduleId: string) => void;
  asBuilt: AsBuiltDump | null;
}

type EcuRow =
  | { kind: "profile"; key: string; scan: ModuleScanResult }
  | { kind: "discovered"; key: string; ecu: DiscoveredEcu };

export const EcuPanel = ({
  status,
  modules,
  discovered,
  discoverProgress,
  busy,
  onScanProfile,
  onDiscover,
  onStop,
  onDumpAsBuilt,
  asBuilt,
}: EcuPanelProps) => {
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [bus, setBus] = useState<"hs" | "ms">("hs");
  const [fromId, setFromId] = useState("0x700");
  const [toId, setToId] = useState("0x7EF");

  const rows: EcuRow[] = useMemo(() => {
    const profileRows: EcuRow[] = modules.map((m) => ({
      kind: "profile",
      key: `p:${m.id}`,
      scan: m,
    }));
    const discRows: EcuRow[] = discovered
      .filter(
        (d) =>
          !modules.some((m) => m.tx_id === d.tx_id && m.bus === d.bus),
      )
      .map((d) => ({
        kind: "discovered",
        key: `d:${d.bus}:${d.tx_id}`,
        ecu: d,
      }));
    return [...profileRows, ...discRows];
  }, [modules, discovered]);

  const selected = rows.find((r) => r.key === selectedKey) ?? rows[0] ?? null;

  const parseHex = (s: string) => {
    const n = Number.parseInt(s.replace(/^0x/i, ""), 16);
    return Number.isFinite(n) ? n : 0x700;
  };

  return (
    <section className="flex min-h-0 flex-1 flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-xl font-semibold">ECUs</h2>
          <p className="text-sm text-[var(--text-muted)]">
            {status?.vin ?? "VIN unknown"} ·{" "}
            {modules.filter((m) => m.online).length}/{modules.length} profile ·{" "}
            {discovered.length} discovered
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            onClick={onScanProfile}
            disabled={!!busy}
            className="rounded bg-[var(--accent)] px-3 py-1.5 text-sm font-semibold text-[var(--bg-deep)] disabled:opacity-40"
          >
            Scan profile
          </button>
          <button
            type="button"
            onClick={onStop}
            disabled={!busy}
            className="rounded border border-[var(--line)] px-3 py-1.5 text-sm hover:bg-[var(--bg-hover)] disabled:opacity-40"
          >
            Stop
          </button>
        </div>
      </div>

      <div className="flex flex-wrap items-end gap-2 rounded border border-[var(--line)] bg-[var(--bg)] p-3 text-sm">
        <label className="flex flex-col gap-1">
          <span className="text-xs text-[var(--text-muted)]">Bus</span>
          <select
            value={bus}
            onChange={(e) => setBus(e.target.value as "hs" | "ms")}
            className="rounded border border-[var(--line)] bg-[var(--bg-elevated)] px-2 py-1.5"
          >
            <option value="hs">HS-CAN</option>
            <option value="ms">MS-CAN</option>
          </select>
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs text-[var(--text-muted)]">From</span>
          <input
            value={fromId}
            onChange={(e) => setFromId(e.target.value)}
            className="mono w-24 rounded border border-[var(--line)] bg-[var(--bg-elevated)] px-2 py-1.5"
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs text-[var(--text-muted)]">To</span>
          <input
            value={toId}
            onChange={(e) => setToId(e.target.value)}
            className="mono w-24 rounded border border-[var(--line)] bg-[var(--bg-elevated)] px-2 py-1.5"
          />
        </label>
        <button
          type="button"
          disabled={!!busy}
          onClick={() => onDiscover(bus, parseHex(fromId), parseHex(toId))}
          className="rounded border border-[var(--line)] px-3 py-1.5 hover:bg-[var(--bg-hover)] disabled:opacity-40"
        >
          Discover bus
        </button>
        {discoverProgress && (
          <span className="mono text-xs text-[var(--text-muted)]">
            {hexId(discoverProgress.current_id)} · found {discoverProgress.found}
          </span>
        )}
      </div>

      <div className="grid min-h-0 flex-1 gap-4 lg:grid-cols-[280px_1fr]">
        <ul className="overflow-auto rounded border border-[var(--line)] bg-[var(--bg)]">
          {rows.map((row) => {
            const online =
              row.kind === "profile" ? row.scan.online : row.ecu.online;
            const title =
              row.kind === "profile"
                ? row.scan.id
                : (row.ecu.profile_id ?? hexId(row.ecu.tx_id));
            const sub =
              row.kind === "profile"
                ? `${hexId(row.scan.tx_id)} · ${row.scan.bus.toUpperCase()}`
                : `${hexId(row.ecu.tx_id)} · ${row.ecu.bus.toUpperCase()} · disc.`;
            return (
              <li key={row.key}>
                <button
                  type="button"
                  onClick={() => setSelectedKey(row.key)}
                  className={`flex w-full items-center gap-3 border-b border-[var(--line)] px-4 py-3 text-left text-sm hover:bg-[var(--bg-hover)] ${
                    (selectedKey ?? rows[0]?.key) === row.key
                      ? "bg-[var(--bg-elevated)]"
                      : ""
                  }`}
                >
                  <span
                    className={`h-2 w-2 shrink-0 rounded-full ${
                      online ? "bg-[var(--accent)]" : "bg-[var(--offline,#5a6b62)]"
                    }`}
                  />
                  <span className="flex-1">
                    <span className="block font-medium">{title}</span>
                    <span className="mono text-xs text-[var(--text-muted)]">
                      {sub}
                    </span>
                  </span>
                </button>
              </li>
            );
          })}
          {rows.length === 0 && (
            <li className="p-4 text-sm text-[var(--text-muted)]">
              Scan the profile or discover the bus.
            </li>
          )}
        </ul>

        <div className="overflow-auto rounded border border-[var(--line)] bg-[var(--bg)] p-5">
          {!selected ? (
            <p className="text-[var(--text-muted)]">Select an ECU</p>
          ) : selected.kind === "profile" ? (
            <ProfileDetail
              scan={selected.scan}
              busy={busy}
              asBuilt={asBuilt}
              onDump={() => onDumpAsBuilt(selected.scan.id)}
            />
          ) : (
            <DiscoveredDetail ecu={selected.ecu} />
          )}
        </div>
      </div>
    </section>
  );
};

const ProfileDetail = ({
  scan,
  busy,
  asBuilt,
  onDump,
}: {
  scan: ModuleScanResult;
  busy: string | null;
  asBuilt: AsBuiltDump | null;
  onDump: () => void;
}) => (
  <div className="space-y-5">
    <div>
      <h3 className="text-lg font-semibold">{scan.name}</h3>
      <p className="mono text-sm text-[var(--text-muted)]">
        {scan.id} · TX {hexId(scan.tx_id)} · RX {hexId(scan.tx_id | 0x08)} ·{" "}
        {scan.bus.toUpperCase()}-CAN
      </p>
      {!scan.online && scan.error && (
        <p className="mt-2 text-sm text-[var(--warn)]">{scan.error}</p>
      )}
    </div>
    {scan.identity.length > 0 && (
      <IdentityList items={scan.identity} />
    )}
    {scan.online && (
      <button
        type="button"
        disabled={!!busy}
        onClick={onDump}
        className="rounded border border-[var(--line)] px-3 py-1.5 text-sm hover:bg-[var(--bg-hover)] disabled:opacity-40"
      >
        Dump As-Built & export
      </button>
    )}
    {asBuilt && asBuilt.module_id === scan.id && (
      <div>
        <h4 className="mb-2 text-xs uppercase tracking-wider text-[var(--text-muted)]">
          As-Built ({asBuilt.blocks.length})
        </h4>
        <ul className="mono space-y-2 text-xs">
          {asBuilt.blocks.map((b) => (
            <li key={b.did} className="break-all rounded bg-[var(--bg-elevated)] p-2">
              <span className="text-[var(--accent)]">{hexId(b.did, 4)}</span>{" "}
              {b.hex}
            </li>
          ))}
        </ul>
        <button
          type="button"
          className="mt-2 text-xs text-[var(--accent)]"
          onClick={() =>
            downloadJson(
              `${asBuilt.module_id}-asbuilt-${asBuilt.exported_at_unix_ms}.json`,
              asBuilt,
            )
          }
        >
          Re-download JSON
        </button>
      </div>
    )}
  </div>
);

const DiscoveredDetail = ({ ecu }: { ecu: DiscoveredEcu }) => (
  <div className="space-y-5">
    <div>
      <h3 className="text-lg font-semibold">
        {ecu.name ?? ecu.profile_id ?? "Unknown ECU"}
      </h3>
      <p className="mono text-sm text-[var(--text-muted)]">
        TX {hexId(ecu.tx_id)} · RX {hexId(ecu.rx_id)} · {ecu.bus.toUpperCase()}-CAN
      </p>
    </div>
    {ecu.identity.length > 0 && <IdentityList items={ecu.identity} />}
  </div>
);

const IdentityList = ({
  items,
}: {
  items: { did: number; ascii: string | null; hex: string }[];
}) => (
  <div>
    <h4 className="mb-2 text-xs uppercase tracking-wider text-[var(--text-muted)]">
      Identity
    </h4>
    <ul className="space-y-1 text-sm">
      {items.map((d) => (
        <li
          key={d.did}
          className="flex justify-between gap-4 border-b border-[var(--line)]/60 py-1"
        >
          <span className="mono text-[var(--text-muted)]">{hexId(d.did, 4)}</span>
          <span className="mono truncate">{d.ascii ?? d.hex}</span>
        </li>
      ))}
    </ul>
  </div>
);
