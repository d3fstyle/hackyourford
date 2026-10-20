import { useMemo } from "react";
import type { LiveSample, VehicleProfile } from "../types";
import { hexId } from "../types";

interface LivePanelProps {
  running: boolean;
  busy: string | null;
  stream: LiveSample[];
  foundDids: LiveSample[];
  foundPids: LiveSample[];
  profile: VehicleProfile | null;
  onStart: (pids: number[], dids: [string, number, string | null][]) => void;
  onStop: () => void;
  onSave: () => void;
  onProbePids: () => Promise<number[]>;
}

export const LivePanel = ({
  running,
  busy,
  stream,
  foundDids,
  foundPids,
  profile,
  onStart,
  onStop,
  onSave,
  onProbePids,
}: LivePanelProps) => {
  const defaultDids = useMemo(() => {
    if (!profile) return [] as [string, number, string | null][];
    return profile.modules.flatMap((m) =>
      m.dids
        .filter((d) => !d.as_built)
        .slice(0, 2)
        .map(
          (d): [string, number, string | null] => [m.id, d.did, d.label],
        ),
    );
  }, [profile]);

  const handleStart = async () => {
    let pids = [0x0c, 0x0d, 0x05, 0x11];
    try {
      const supported = await onProbePids();
      if (supported.length > 0) {
        pids = supported.filter((p) =>
          [0x0c, 0x0d, 0x05, 0x0f, 0x11, 0x04, 0x42].includes(p),
        );
        if (pids.length === 0) pids = supported.slice(0, 6);
      }
    } catch {
      // keep defaults
    }
    onStart(pids, defaultDids);
  };

  return (
    <section className="flex min-h-0 flex-1 flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-xl font-semibold">Live</h2>
          <p className="text-sm text-[var(--text-muted)]">
            Real-time stream · UDS DIDs and Mode 01 PIDs
          </p>
        </div>
        <div className="flex gap-2">
          {!running ? (
            <button
              type="button"
              disabled={!!busy}
              onClick={() => void handleStart()}
              className="rounded bg-[var(--accent)] px-3 py-1.5 text-sm font-semibold text-[var(--bg-deep)] disabled:opacity-40"
            >
              Start
            </button>
          ) : (
            <button
              type="button"
              onClick={onStop}
              className="rounded border border-[var(--line)] px-3 py-1.5 text-sm hover:bg-[var(--bg-hover)]"
            >
              Stop
            </button>
          )}
          <button
            type="button"
            disabled={stream.length === 0}
            onClick={onSave}
            className="rounded border border-[var(--line)] px-3 py-1.5 text-sm hover:bg-[var(--bg-hover)] disabled:opacity-40"
          >
            Save session
          </button>
        </div>
      </div>

      <div className="grid min-h-0 flex-1 gap-4 lg:grid-cols-2">
        <div className="flex min-h-0 flex-col rounded border border-[var(--line)] bg-[var(--bg)]">
          <div className="border-b border-[var(--line)] px-3 py-2 text-xs uppercase tracking-wider text-[var(--text-muted)]">
            Stream ({stream.length})
          </div>
          <ul className="mono min-h-0 flex-1 overflow-auto p-2 text-xs">
            {[...stream].slice(-200).reverse().map((s, i) => (
              <li
                key={`${s.ts_ms}-${s.source}-${s.id}-${i}`}
                className="border-b border-[var(--line)]/40 py-1"
              >
                <span className="text-[var(--text-muted)]">{s.ts_ms}</span>{" "}
                <span className="text-[var(--accent)]">
                  {s.source === "pid" ? `PID ${s.id.toString(16).padStart(2, "0").toUpperCase()}` : `DID ${hexId(s.id, 4)}`}
                </span>{" "}
                {s.decoded ?? s.raw_hex}
              </li>
            ))}
            {stream.length === 0 && (
              <li className="p-2 text-[var(--text-muted)]">No samples yet</li>
            )}
          </ul>
        </div>

        <div className="grid min-h-0 grid-rows-2 gap-4">
          <FoundList title="UDS DIDs found" items={foundDids} kind="did" />
          <FoundList title="OBD Mode 01 PIDs" items={foundPids} kind="pid" />
        </div>
      </div>
    </section>
  );
};

const FoundList = ({
  title,
  items,
  kind,
}: {
  title: string;
  items: LiveSample[];
  kind: "did" | "pid";
}) => (
  <div className="flex min-h-0 flex-col rounded border border-[var(--line)] bg-[var(--bg)]">
    <div className="border-b border-[var(--line)] px-3 py-2 text-xs uppercase tracking-wider text-[var(--text-muted)]">
      {title} ({items.length})
    </div>
    <ul className="min-h-0 flex-1 overflow-auto p-2 text-sm">
      {items.map((s) => (
        <li
          key={`${kind}-${s.id}`}
          className="flex justify-between gap-2 border-b border-[var(--line)]/40 py-1.5"
        >
          <span className="mono text-[var(--accent)]">
            {kind === "pid"
              ? `0x${s.id.toString(16).padStart(2, "0").toUpperCase()}`
              : hexId(s.id, 4)}
            {s.label ? ` · ${s.label}` : ""}
          </span>
          <span className="mono truncate text-[var(--text-muted)]">
            {s.decoded ?? s.raw_hex}
          </span>
        </li>
      ))}
      {items.length === 0 && (
        <li className="p-2 text-sm text-[var(--text-muted)]">—</li>
      )}
    </ul>
  </div>
);
