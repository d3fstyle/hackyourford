import { useEffect, useMemo, useState } from "react";
import type {
  AsBuiltBlockWrite,
  AsBuiltDump,
  ConnectionStatus,
  DidValue,
  DiscoveredEcu,
  EcuDiscoverProgress,
  ModuleScanResult,
  ParamDef,
  ParamEdit,
  ParamValue,
  VehicleProfile,
} from "../types";
import { hexId } from "../types";
import { downloadJson } from "../utils/export";
import {
  bytesToHex,
  getBits,
  hexToBytes,
  setBits,
} from "../utils/asBuiltBits";

interface EcuPanelProps {
  status: ConnectionStatus | null;
  modules: ModuleScanResult[];
  discovered: DiscoveredEcu[];
  discoverProgress: EcuDiscoverProgress | null;
  busy: string | null;
  profile: VehicleProfile | null;
  onScanProfile: () => void;
  onDiscover: (bus: "hs" | "ms", fromId: number, toId: number) => void;
  onStop: () => void;
  onDumpAsBuilt: (moduleId: string) => void;
  onWriteAsBuilt: (moduleId: string, blocks: AsBuiltBlockWrite[]) => void;
  onWriteAsBuiltParams: (moduleId: string, edits: ParamEdit[]) => void;
  asBuilt: AsBuiltDump | null;
  paramValues: ParamValue[] | null;
  sweptDids: DidValue[];
  didDiffs: [number, string, string][];
  onDidSweep: (moduleId: string, from: number, to: number) => void;
  onSnapshot: (
    moduleId: string,
    from: number,
    to: number,
    which: "before" | "after",
  ) => void;
  onDiff: () => void;
  onPinDid: (moduleId: string, did: number, label?: string) => void;
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
  profile,
  onScanProfile,
  onDiscover,
  onStop,
  onDumpAsBuilt,
  onWriteAsBuilt,
  onWriteAsBuiltParams,
  asBuilt,
  paramValues,
  sweptDids,
  didDiffs,
  onDidSweep,
  onSnapshot,
  onDiff,
  onPinDid,
}: EcuPanelProps) => {
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [bus, setBus] = useState<"hs" | "ms">("hs");
  const [fromId, setFromId] = useState("0x700");
  const [toId, setToId] = useState("0x7EF");
  const [didMode, setDidMode] = useState<"sweep" | "snapshot" | "diff" | null>(
    null,
  );
  const [fromDid, setFromDid] = useState("0xF190");
  const [toDid, setToDid] = useState("0xF1FF");

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

  const moduleParams =
    selected?.kind === "profile"
      ? (profile?.modules.find((m) => m.id === selected.scan.id)?.params ?? [])
      : [];

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
            className="rounded border px-2 py-1.5 text-sm"
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
              paramDefs={moduleParams}
              paramValues={
                asBuilt?.module_id === selected.scan.id ? paramValues : null
              }
              onDump={() => onDumpAsBuilt(selected.scan.id)}
              onWrite={(blocks) => onWriteAsBuilt(selected.scan.id, blocks)}
              onWriteParams={(edits) =>
                onWriteAsBuiltParams(selected.scan.id, edits)
              }
            />
          ) : (
            <DiscoveredDetail ecu={selected.ecu}             />
          )}

          {selected?.kind === "profile" && (
            <div className="mt-4 border-t border-[var(--line)] pt-4">
              <div className="mb-3 flex items-center justify-between">
                <h3 className="font-semibold">DID Tools</h3>
                <div className="flex gap-2">
                  <button
                    type="button"
                    onClick={() => setDidMode(didMode === "sweep" ? null : "sweep")}
                    className={`rounded px-3 py-1 text-sm ${
                      didMode === "sweep"
                        ? "bg-[var(--accent)] text-[var(--bg-deep)]"
                        : "border border-[var(--line)]"
                    }`}
                  >
                    Sweep
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      setDidMode(didMode === "snapshot" ? null : "snapshot")
                    }
                    className={`rounded px-3 py-1 text-sm ${
                      didMode === "snapshot"
                        ? "bg-[var(--accent)] text-[var(--bg-deep)]"
                        : "border border-[var(--line)]"
                    }`}
                  >
                    Snapshot
                  </button>
                  <button
                    type="button"
                    onClick={() => setDidMode(didMode === "diff" ? null : "diff")}
                    className={`rounded px-3 py-1 text-sm ${
                      didMode === "diff"
                        ? "bg-[var(--accent)] text-[var(--bg-deep)]"
                        : "border border-[var(--line)]"
                    }`}
                  >
                    Diff
                  </button>
                </div>
              </div>

              {didMode === "sweep" && (
                <div className="space-y-3">
                  <div className="flex gap-2">
                    <input
                      placeholder="From DID (0xF190)"
                      value={fromDid}
                      onChange={(e) => setFromDid(e.target.value)}
                      className="mono flex-1 rounded border border-[var(--line)] bg-[var(--bg-elevated)] px-3 py-2 text-sm"
                    />
                    <input
                      placeholder="To DID (0xF1FF)"
                      value={toDid}
                      onChange={(e) => setToDid(e.target.value)}
                      className="mono flex-1 rounded border border-[var(--line)] bg-[var(--bg-elevated)] px-3 py-2 text-sm"
                    />
                    <button
                      type="button"
                      disabled={!!busy || selected.kind !== "profile"}
                      onClick={() =>
                        selected.kind === "profile" &&
                        onDidSweep(
                          selected.scan.id,
                          parseHex(fromDid),
                          parseHex(toDid),
                        )
                      }
                      className="rounded bg-[var(--accent)] px-4 py-2 text-sm font-semibold text-[var(--bg-deep)] disabled:opacity-40"
                    >
                      Sweep
                    </button>
                  </div>
                  <div className="max-h-60 overflow-auto rounded border border-[var(--line)] bg-[var(--bg)]">
                    <div className="sticky top-0 border-b border-[var(--line)] bg-[var(--bg-elevated)] px-3 py-2 text-xs uppercase text-[var(--text-muted)]">
                      Found ({sweptDids.length})
                    </div>
                    <ul className="p-2 text-sm">
                      {sweptDids.map((d) => (
                        <li
                          key={d.did}
                          className="flex items-center justify-between gap-2 border-b border-[var(--line)]/40 py-1.5"
                        >
                          <span className="mono">
                            {hexId(d.did, 4)} · {d.ascii ?? d.hex.slice(0, 24)}
                          </span>
                          <button
                            type="button"
                            className="text-xs text-[var(--accent)]"
                            onClick={() =>
                              selected.kind === "profile" &&
                              onPinDid(
                                selected.scan.id,
                                d.did,
                                d.ascii ?? undefined,
                              )
                            }
                          >
                            Pin to OBD
                          </button>
                        </li>
                      ))}
                    </ul>
                  </div>
                </div>
              )}

              {didMode === "snapshot" && (
                <div className="space-y-3">
                  <div className="flex gap-2">
                    <input
                      placeholder="From DID (0xF190)"
                      value={fromDid}
                      onChange={(e) => setFromDid(e.target.value)}
                      className="mono flex-1 rounded border border-[var(--line)] bg-[var(--bg-elevated)] px-3 py-2 text-sm"
                    />
                    <input
                      placeholder="To DID (0xF1FF)"
                      value={toDid}
                      onChange={(e) => setToDid(e.target.value)}
                      className="mono flex-1 rounded border border-[var(--line)] bg-[var(--bg-elevated)] px-3 py-2 text-sm"
                    />
                    <button
                      type="button"
                      disabled={!!busy || selected.kind !== "profile"}
                      onClick={() =>
                        selected.kind === "profile" &&
                        onSnapshot(
                          selected.scan.id,
                          parseHex(fromDid),
                          parseHex(toDid),
                          "before",
                        )
                      }
                      className="rounded border border-[var(--line)] px-3 py-2 text-sm"
                    >
                      Snapshot A
                    </button>
                    <button
                      type="button"
                      disabled={!!busy || selected.kind !== "profile"}
                      onClick={() =>
                        selected.kind === "profile" &&
                        onSnapshot(
                          selected.scan.id,
                          parseHex(fromDid),
                          parseHex(toDid),
                          "after",
                        )
                      }
                      className="rounded border border-[var(--line)] px-3 py-2 text-sm"
                    >
                      Snapshot B
                    </button>
                  </div>
                  <p className="text-xs text-[var(--text-muted)]">
                    Take snapshot A, make a change, then take snapshot B to compare.
                  </p>
                </div>
              )}

              {didMode === "diff" && (
                <div className="space-y-3">
                  <button
                    type="button"
                    disabled={!!busy}
                    onClick={onDiff}
                    className="rounded bg-[var(--accent)] px-4 py-2 text-sm font-semibold text-[var(--bg-deep)] disabled:opacity-40"
                  >
                    Compare A → B
                  </button>
                  <div className="max-h-60 overflow-auto rounded border border-[var(--line)] bg-[var(--bg)]">
                    <div className="sticky top-0 border-b border-[var(--line)] bg-[var(--bg-elevated)] px-3 py-2 text-xs uppercase text-[var(--text-muted)]">
                      Differences ({didDiffs.length})
                    </div>
                    <ul className="mono p-2 text-xs">
                      {didDiffs.map(([did, a, b]) => (
                        <li key={did} className="border-b border-[var(--line)]/40 py-1">
                          {hexId(did, 4)}: {a || "∅"} → {b}
                        </li>
                      ))}
                    </ul>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </section>
  );
};

const parseHexPayload = (hex: string): number[] | null => hexToBytes(hex);

const ProfileDetail = ({
  scan,
  busy,
  asBuilt,
  paramDefs,
  paramValues,
  onDump,
  onWrite,
  onWriteParams,
}: {
  scan: ModuleScanResult;
  busy: string | null;
  asBuilt: AsBuiltDump | null;
  paramDefs: ParamDef[];
  paramValues: ParamValue[] | null;
  onDump: () => void;
  onWrite: (blocks: AsBuiltBlockWrite[]) => void;
  onWriteParams: (edits: ParamEdit[]) => void;
}) => {
  const [edits, setEdits] = useState<Record<number, string>>({});
  const [paramState, setParamState] = useState<Record<string, number>>({});
  const [paramDirty, setParamDirty] = useState(false);

  useEffect(() => {
    if (!asBuilt || asBuilt.module_id !== scan.id) {
      setEdits({});
      setParamState({});
      setParamDirty(false);
      return;
    }
    const next: Record<number, string> = {};
    for (const b of asBuilt.blocks) {
      next[b.did] = b.hex;
    }
    setEdits(next);
    setParamDirty(false);
  }, [asBuilt, scan.id]);

  useEffect(() => {
    if (!paramValues || !asBuilt || asBuilt.module_id !== scan.id) {
      return;
    }
    const next: Record<string, number> = {};
    for (const v of paramValues) {
      next[v.id] = v.value;
    }
    setParamState(next);
  }, [paramValues, asBuilt, scan.id]);

  const showEditor = asBuilt && asBuilt.module_id === scan.id;

  const applyParamToHex = (def: ParamDef, value: number) => {
    setParamState((prev) => ({ ...prev, [def.id]: value }));
    setParamDirty(true);
    setEdits((prev) => {
      const hex = prev[def.did];
      if (!hex) return prev;
      const bytes = hexToBytes(hex);
      if (!bytes) return prev;
      const updated = setBits(bytes, def.byte, def.bit, def.width ?? 1, value);
      if (!updated) return prev;
      return { ...prev, [def.did]: bytesToHex(updated) };
    });
  };

  return (
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
      {scan.identity.length > 0 && <IdentityList items={scan.identity} />}
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

      {showEditor && asBuilt && paramDefs.length > 0 && (
        <div>
          <h4 className="mb-2 text-xs uppercase tracking-wider text-[var(--text-muted)]">
            Parameters ({paramDefs.length})
          </h4>
          <ul className="space-y-3">
            {paramDefs.map((def) => {
              const value =
                paramState[def.id] ??
                (() => {
                  const hex = edits[def.did];
                  const bytes = hex ? hexToBytes(hex) : null;
                  if (!bytes) return 0;
                  return getBits(bytes, def.byte, def.bit, def.width ?? 1) ?? 0;
                })();
              const verified = def.verified ?? false;
              return (
                <li
                  key={def.id}
                  className="rounded bg-[var(--bg-elevated)] p-3 text-sm"
                >
                  <div className="mb-2 flex flex-wrap items-center gap-2">
                    <span className="font-medium">{def.label}</span>
                    {!verified && (
                      <span className="rounded border border-[var(--warn)] px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-[var(--warn)]">
                        unverified
                      </span>
                    )}
                    <span className="mono text-xs text-[var(--text-muted)]">
                      {hexId(def.did, 4)} byte{def.byte} bit{def.bit}
                    </span>
                  </div>
                  {def.kind === "bool" ? (
                    <label className="flex items-center gap-2">
                      <input
                        type="checkbox"
                        disabled={!!busy}
                        checked={value !== 0}
                        onChange={(e) =>
                          applyParamToHex(def, e.target.checked ? 1 : 0)
                        }
                      />
                      <span>{value ? "On" : "Off"}</span>
                    </label>
                  ) : def.kind === "enum" ? (
                    <select
                      disabled={!!busy}
                      value={value}
                      onChange={(e) =>
                        applyParamToHex(def, Number(e.target.value))
                      }
                      className="rounded border px-2 py-1.5 text-sm"
                    >
                      {(def.options ?? []).map((o) => (
                        <option key={o.value} value={o.value}>
                          {o.label}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <input
                      type="number"
                      disabled={!!busy}
                      min={0}
                      max={(1 << (def.width ?? 1)) - 1}
                      value={value}
                      onChange={(e) =>
                        applyParamToHex(def, Number(e.target.value) || 0)
                      }
                      className="mono w-28 rounded border border-[var(--line)] bg-[var(--bg)] px-2 py-1.5"
                    />
                  )}
                  {def.notes && (
                    <p className="mt-1 text-xs text-[var(--text-muted)]">
                      {def.notes}
                    </p>
                  )}
                </li>
              );
            })}
          </ul>
          <button
            type="button"
            disabled={!!busy || !paramDirty}
            className="mt-3 rounded border border-[var(--warn)] px-3 py-1.5 text-sm text-[var(--warn)] hover:bg-[var(--bg-hover)] disabled:opacity-40"
            onClick={() => {
              const editsList: ParamEdit[] = paramDefs.map((d) => ({
                id: d.id,
                value: paramState[d.id] ?? 0,
              }));
              const hasUnverified = paramDefs.some((d) => !d.verified);
              const msg = hasUnverified
                ? `Write ${editsList.length} parameter(s) to ${scan.id}?\n\nSome parameters are UNVERIFIED — bit maps may be wrong for your car.`
                : `Write ${editsList.length} parameter(s) to ${scan.id}? Incorrect values can break vehicle configuration.`;
              if (!window.confirm(msg)) return;
              onWriteParams(editsList);
              setParamDirty(false);
            }}
          >
            Write parameters
          </button>
        </div>
      )}

      {showEditor && asBuilt && (
        <div>
          <h4 className="mb-2 text-xs uppercase tracking-wider text-[var(--text-muted)]">
            As-Built hex ({asBuilt.blocks.length})
          </h4>
          <ul className="space-y-3 text-xs">
            {asBuilt.blocks.map((b) => (
              <li key={b.did} className="rounded bg-[var(--bg-elevated)] p-2">
                <label className="mb-1 block mono text-[var(--accent)]">
                  {hexId(b.did, 4)}
                </label>
                <textarea
                  className="mono w-full resize-y rounded border border-[var(--line)] bg-[var(--bg)] p-2 text-xs"
                  rows={2}
                  spellCheck={false}
                  disabled={!!busy}
                  value={edits[b.did] ?? b.hex}
                  onChange={(e) => {
                    setEdits((prev) => ({ ...prev, [b.did]: e.target.value }));
                    setParamDirty(true);
                  }}
                />
              </li>
            ))}
          </ul>
          <div className="mt-3 flex flex-wrap items-center gap-3">
            <button
              type="button"
              disabled={!!busy}
              className="rounded border border-[var(--warn)] px-3 py-1.5 text-sm text-[var(--warn)] hover:bg-[var(--bg-hover)] disabled:opacity-40"
              onClick={() => {
                const blocks: AsBuiltBlockWrite[] = [];
                for (const b of asBuilt.blocks) {
                  const raw = edits[b.did] ?? b.hex;
                  const data = parseHexPayload(raw);
                  if (!data) {
                    window.alert(`Invalid hex for DID ${hexId(b.did, 4)}`);
                    return;
                  }
                  blocks.push({ did: b.did, data });
                }
                const ok = window.confirm(
                  `Write ${blocks.length} As-Built block(s) to ${scan.id}? Incorrect values can break vehicle configuration.`,
                );
                if (!ok) return;
                onWrite(blocks);
              }}
            >
              Write As-Built
            </button>
            <button
              type="button"
              className="text-xs text-[var(--accent)]"
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
        </div>
      )}
    </div>
  );
};

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
