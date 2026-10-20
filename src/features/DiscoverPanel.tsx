import { useState } from "react";
import type {
  CanFrame,
  DidValue,
  PatternCandidate,
  VehicleProfile,
} from "../types";
import { hexId } from "../types";

interface DiscoverPanelProps {
  profile: VehicleProfile | null;
  busy: string | null;
  sniffing: boolean;
  frames: CanFrame[];
  candidates: PatternCandidate[];
  sweptDids: DidValue[];
  didDiffs: [number, string, string][];
  onStartSniff: (bus: "hs" | "ms") => void;
  onStop: () => void;
  onMarkPress: () => void;
  onAnalyze: () => void;
  onDidSweep: (moduleId: string, from: number, to: number) => void;
  onSnapshot: (moduleId: string, from: number, to: number, which: "before" | "after") => void;
  onDiff: () => void;
  onPinDid: (moduleId: string, did: number) => void;
}

export const DiscoverPanel = ({
  profile,
  busy,
  sniffing,
  frames,
  candidates,
  sweptDids,
  didDiffs,
  onStartSniff,
  onStop,
  onMarkPress,
  onAnalyze,
  onDidSweep,
  onSnapshot,
  onDiff,
  onPinDid,
}: DiscoverPanelProps) => {
  const [mode, setMode] = useState<"sniff" | "did">("sniff");
  const [bus, setBus] = useState<"hs" | "ms">("ms");
  const [moduleId, setModuleId] = useState(profile?.modules[0]?.id ?? "PCM");
  const [fromDid, setFromDid] = useState("0xF190");
  const [toDid, setToDid] = useState("0xF1FF");

  const parseHex = (s: string) => {
    const n = Number.parseInt(s.replace(/^0x/i, ""), 16);
    return Number.isFinite(n) ? n : 0;
  };

  return (
    <section className="flex min-h-0 flex-1 flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-xl font-semibold">Discover</h2>
          <p className="text-sm text-[var(--text-muted)]">
            CAN sniffer + press-pattern correlation · UDS DID sweep
          </p>
        </div>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={() => setMode("sniff")}
            className={`rounded px-3 py-1.5 text-sm ${
              mode === "sniff"
                ? "bg-[var(--accent)] font-semibold text-[var(--bg-deep)]"
                : "border border-[var(--line)]"
            }`}
          >
            Bus sniff
          </button>
          <button
            type="button"
            onClick={() => setMode("did")}
            className={`rounded px-3 py-1.5 text-sm ${
              mode === "did"
                ? "bg-[var(--accent)] font-semibold text-[var(--bg-deep)]"
                : "border border-[var(--line)]"
            }`}
          >
            DID sweep
          </button>
        </div>
      </div>

      {mode === "sniff" ? (
        <div className="flex min-h-0 flex-1 flex-col gap-4">
          <div className="flex flex-wrap items-center gap-2">
            <select
              value={bus}
              onChange={(e) => setBus(e.target.value as "hs" | "ms")}
              className="rounded border border-[var(--line)] bg-[var(--bg-elevated)] px-2 py-1.5 text-sm"
              disabled={sniffing}
            >
              <option value="hs">HS-CAN</option>
              <option value="ms">MS-CAN</option>
            </select>
            {!sniffing ? (
              <button
                type="button"
                disabled={!!busy}
                onClick={() => onStartSniff(bus)}
                className="rounded bg-[var(--accent)] px-3 py-1.5 text-sm font-semibold text-[var(--bg-deep)] disabled:opacity-40"
              >
                Start sniff
              </button>
            ) : (
              <button
                type="button"
                onClick={onStop}
                className="rounded border border-[var(--line)] px-3 py-1.5 text-sm"
              >
                Stop
              </button>
            )}
            <button
              type="button"
              disabled={!sniffing}
              onClick={onMarkPress}
              className="rounded border border-[var(--line)] px-3 py-1.5 text-sm hover:bg-[var(--bg-hover)] disabled:opacity-40"
            >
              Mark press
            </button>
            <button
              type="button"
              disabled={frames.length === 0}
              onClick={onAnalyze}
              className="rounded border border-[var(--line)] px-3 py-1.5 text-sm hover:bg-[var(--bg-hover)] disabled:opacity-40"
            >
              Analyze pattern
            </button>
            <span className="text-xs text-[var(--text-muted)]">
              {frames.length} frames
            </span>
          </div>

          <div className="grid min-h-0 flex-1 gap-4 lg:grid-cols-2">
            <div className="flex min-h-0 flex-col rounded border border-[var(--line)] bg-[var(--bg)]">
              <div className="border-b border-[var(--line)] px-3 py-2 text-xs uppercase text-[var(--text-muted)]">
                Frames
              </div>
              <ul className="mono min-h-0 flex-1 overflow-auto p-2 text-xs">
                {[...frames].slice(-150).reverse().map((f, i) => (
                  <li key={`${f.ts_ms}-${f.id}-${i}`} className="py-0.5">
                    <span className="text-[var(--accent)]">{hexId(f.id)}</span>{" "}
                    {f.data
                      .map((b) => b.toString(16).padStart(2, "0"))
                      .join(" ")
                      .toUpperCase()}
                  </li>
                ))}
              </ul>
            </div>
            <div className="flex min-h-0 flex-col rounded border border-[var(--line)] bg-[var(--bg)]">
              <div className="border-b border-[var(--line)] px-3 py-2 text-xs uppercase text-[var(--text-muted)]">
                Candidates ({candidates.length})
              </div>
              <ul className="min-h-0 flex-1 overflow-auto p-2 text-sm">
                {candidates.map((c) => (
                  <li
                    key={`${c.can_id}-${c.byte_index}-${c.bit_index}`}
                    className="border-b border-[var(--line)]/40 py-2"
                  >
                    <div className="mono">
                      ID {hexId(c.can_id)} · byte {c.byte_index}
                      {c.bit_index != null ? ` · bit ${c.bit_index}` : ""} · score{" "}
                      {c.change_score.toFixed(2)}
                    </div>
                    <div className="mono text-xs text-[var(--text-muted)]">
                      {c.sample_values.slice(0, 3).join(" · ")}
                    </div>
                  </li>
                ))}
                {candidates.length === 0 && (
                  <li className="p-2 text-sm text-[var(--text-muted)]">
                    Press the button several times with “Mark press” (start/end
                    of each burst), then analyze.
                  </li>
                )}
              </ul>
            </div>
          </div>
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-4">
          <div className="flex flex-wrap items-end gap-2">
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-xs text-[var(--text-muted)]">Module</span>
              <select
                value={moduleId}
                onChange={(e) => setModuleId(e.target.value)}
                className="rounded border border-[var(--line)] bg-[var(--bg-elevated)] px-2 py-1.5"
              >
                {(profile?.modules ?? []).map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.id}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-xs text-[var(--text-muted)]">From DID</span>
              <input
                value={fromDid}
                onChange={(e) => setFromDid(e.target.value)}
                className="mono w-28 rounded border border-[var(--line)] bg-[var(--bg-elevated)] px-2 py-1.5"
              />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-xs text-[var(--text-muted)]">To DID</span>
              <input
                value={toDid}
                onChange={(e) => setToDid(e.target.value)}
                className="mono w-28 rounded border border-[var(--line)] bg-[var(--bg-elevated)] px-2 py-1.5"
              />
            </label>
            <button
              type="button"
              disabled={!!busy}
              onClick={() =>
                onDidSweep(moduleId, parseHex(fromDid), parseHex(toDid))
              }
              className="rounded bg-[var(--accent)] px-3 py-1.5 text-sm font-semibold text-[var(--bg-deep)] disabled:opacity-40"
            >
              Sweep
            </button>
            <button
              type="button"
              disabled={!!busy}
              onClick={() =>
                onSnapshot(moduleId, parseHex(fromDid), parseHex(toDid), "before")
              }
              className="rounded border border-[var(--line)] px-3 py-1.5 text-sm"
            >
              Snapshot A
            </button>
            <button
              type="button"
              disabled={!!busy}
              onClick={() =>
                onSnapshot(moduleId, parseHex(fromDid), parseHex(toDid), "after")
              }
              className="rounded border border-[var(--line)] px-3 py-1.5 text-sm"
            >
              Snapshot B
            </button>
            <button
              type="button"
              onClick={onDiff}
              className="rounded border border-[var(--line)] px-3 py-1.5 text-sm"
            >
              Diff A/B
            </button>
            <button
              type="button"
              onClick={onStop}
              className="rounded border border-[var(--line)] px-3 py-1.5 text-sm"
            >
              Stop
            </button>
          </div>

          <div className="grid min-h-0 flex-1 gap-4 lg:grid-cols-2">
            <div className="flex min-h-0 flex-col rounded border border-[var(--line)] bg-[var(--bg)]">
              <div className="border-b border-[var(--line)] px-3 py-2 text-xs uppercase text-[var(--text-muted)]">
                DIDs found ({sweptDids.length})
              </div>
              <ul className="min-h-0 flex-1 overflow-auto p-2 text-sm">
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
                      onClick={() => onPinDid(moduleId, d.did)}
                    >
                      Pin to live
                    </button>
                  </li>
                ))}
              </ul>
            </div>
            <div className="flex min-h-0 flex-col rounded border border-[var(--line)] bg-[var(--bg)]">
              <div className="border-b border-[var(--line)] px-3 py-2 text-xs uppercase text-[var(--text-muted)]">
                Diff A→B ({didDiffs.length})
              </div>
              <ul className="mono min-h-0 flex-1 overflow-auto p-2 text-xs">
                {didDiffs.map(([did, a, b]) => (
                  <li key={did} className="border-b border-[var(--line)]/40 py-1">
                    {hexId(did, 4)}: {a || "∅"} → {b}
                  </li>
                ))}
              </ul>
            </div>
          </div>
        </div>
      )}
    </section>
  );
};
