import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Button } from "../../components/Button";
import { PanelHeader } from "../../components/PanelHeader";
import { EmptyState } from "../../components/EmptyState";
import { Select } from "../../components/Select";
import type {
  LiveSample,
  LiveDidPin,
  ObdDashboard,
  ObdDashboardSignalKey,
  FordExtendedPidRow,
  VehicleProfile,
} from "../../types";
import { downloadCsv } from "../../utils/export";

interface ObdPanelProps {
  running: boolean;
  busy: string | null;
  profile: VehicleProfile | null;
  pinnedDids: LiveDidPin[];
  onStart: (pids: number[], dids: LiveDidPin[]) => void;
  onStop: () => void;
}

interface ObdPidMeta {
  pid: number;
  label: string;
  unit?: string;
}

interface CatalogItem {
  source: "pid" | "did" | "ford";
  id: number;
  module_id?: string;
  label: string;
  unit?: string;
  equation?: string;
  scan_tool?: boolean;
}

const makeKey = (item: CatalogItem) =>
  `${item.source}:${item.id}:${item.module_id ?? ""}`;

const parseMode22Did = (modeAndPid: string): number | null => {
  const hex = modeAndPid.replace(/[^0-9a-fA-F]/g, "").toUpperCase();
  if (hex.length < 4) return null;
  if (!hex.startsWith("22")) return null;
  const did = Number.parseInt(hex.slice(2), 16);
  return Number.isFinite(did) ? did : null;
};

export const ObdPanel = ({
  running,
  busy,
  profile,
  pinnedDids,
  onStart,
  onStop,
}: ObdPanelProps) => {
  const [catalog, setCatalog] = useState<CatalogItem[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [dashboards, setDashboards] = useState<ObdDashboard[]>([]);
  const [activeDashboard, setActiveDashboard] = useState<string | null>(null);
  const [liveData, setLiveData] = useState<Map<string, LiveSample>>(new Map());
  const [recording, setRecording] = useState(false);
  const [recordedData, setRecordedData] = useState<LiveSample[]>([]);
  const [chartSignal, setChartSignal] = useState<string | null>(null);
  const [chartHistory, setChartHistory] = useState<number[]>([]);
  const [filterText, setFilterText] = useState("");

  const loadCatalog = useCallback(async () => {
    const items: CatalogItem[] = [];

    try {
      const [supported, meta, scanSupported] = await Promise.all([
        invoke<number[]>("list_supported_pids").catch(() => [] as number[]),
        invoke<ObdPidMeta[]>("list_mode01_pid_meta").catch(() => [] as ObdPidMeta[]),
        invoke<number[]>("list_mode01_scan_supported").catch(() => [] as number[]),
      ]);
      const scanSet = new Set(scanSupported);
      const pidList = supported.length > 0 ? supported : meta.map((m) => m.pid).slice(0, 40);
      for (const pid of pidList) {
        const m = meta.find((x) => x.pid === pid);
        items.push({
          source: "pid",
          id: pid,
          label: m?.label ?? `PID 0x${pid.toString(16).toUpperCase()}`,
          unit: m?.unit,
          scan_tool: scanSet.has(pid) || supported.includes(pid),
        });
      }
    } catch {
      // ignore
    }

    if (profile) {
      for (const mod of profile.modules) {
        for (const did of mod.dids) {
          if (did.as_built) continue;
          items.push({
            source: "did",
            id: did.did,
            module_id: mod.id,
            label: did.label || `${mod.id} DID`,
          });
        }
      }
    }

    try {
      const ford = await invoke<FordExtendedPidRow[]>("list_ford_extended_pids", {
        profileId: profile?.id ?? null,
      });
      for (const f of ford) {
        const did = parseMode22Did(f.mode_and_pid);
        if (did == null) continue;
        items.push({
          source: "ford",
          id: did,
          module_id: f.module_id,
          label: f.short_name || f.name,
          unit: f.unit,
          equation: f.equation,
        });
      }
    } catch {
      // ignore
    }

    for (const pin of pinnedDids) {
      const exists = items.some(
        (i) =>
          (i.source === "did" || i.source === "ford") &&
          i.id === pin.did &&
          i.module_id === pin.module_id,
      );
      if (exists) continue;
      items.push({
        source: "did",
        id: pin.did,
        module_id: pin.module_id,
        label: pin.label ?? `DID 0x${pin.did.toString(16).toUpperCase()}`,
        unit: pin.unit,
        equation: pin.equation,
      });
    }

    setCatalog(items);
  }, [profile, pinnedDids]);

  const loadDashboards = useCallback(async () => {
    try {
      const [global, profileDash] = await Promise.all([
        invoke<ObdDashboard[]>("list_obd_dashboards", { scope: "global" }),
        profile
          ? invoke<ObdDashboard[]>("list_obd_dashboards", {
              scope: "profile",
              profileId: profile.id,
            })
          : Promise.resolve([] as ObdDashboard[]),
      ]);
      setDashboards([...profileDash, ...global]);
    } catch {
      const stored = localStorage.getItem("obd_dashboards");
      if (stored) setDashboards(JSON.parse(stored) as ObdDashboard[]);
    }
  }, [profile]);

  useEffect(() => {
    void loadCatalog();
    void loadDashboards();
  }, [loadCatalog, loadDashboards]);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    void (async () => {
      unsub = await listen<LiveSample>("live.sample", (e) => {
        const key = `${e.payload.source}:${e.payload.id}:${e.payload.module_id ?? ""}`;
        setLiveData((prev) => new Map(prev).set(key, e.payload));
        if (recording) {
          setRecordedData((prev) => [...prev, e.payload]);
        }
        if (chartSignal) {
          const chartKey = chartSignal;
          const sampleKey = `${e.payload.source}:${e.payload.id}:${e.payload.module_id ?? ""}`;
          if (chartKey.includes(String(e.payload.id)) || sampleKey === chartKey) {
            const v = e.payload.value;
            if (v != null) {
              setChartHistory((prev) => [...prev.slice(-119), v]);
            }
          }
        }
      });
    })();
    return () => unsub?.();
  }, [recording, chartSignal]);

  const saveDashboard = async (name: string) => {
    const signals: ObdDashboardSignalKey[] = Array.from(selected)
      .map((key) => {
        const item = catalog.find((c) => makeKey(c) === key);
        if (!item) return null;
        if (item.source === "pid") {
          return {
            source: "pid",
            id: item.id.toString(16).toUpperCase().padStart(2, "0"),
          };
        }
        return {
          source: item.source === "ford" ? "ford_extended" : "did",
          id: `${item.module_id ?? "PCM"}:${item.id}`,
        };
      })
      .filter((s): s is ObdDashboardSignalKey => s !== null);

    const dashboard: ObdDashboard = {
      id: `dash_${Date.now()}`,
      name,
      scope: profile ? "profile" : "global",
      profile_id: profile?.id ?? null,
      signals,
    };

    try {
      await invoke("save_obd_dashboard", { dashboard });
      await loadDashboards();
    } catch {
      const updated = [...dashboards, dashboard];
      setDashboards(updated);
      localStorage.setItem("obd_dashboards", JSON.stringify(updated));
    }
  };

  const applyDashboard = (dashboard: ObdDashboard) => {
    const keys = dashboard.signals
      .map((s) => {
        if (s.source === "pid") {
          const pid = Number.parseInt(s.id, 16);
          const item = catalog.find((c) => c.source === "pid" && c.id === pid);
          return item ? makeKey(item) : null;
        }
        const [moduleId, idStr] = s.id.split(":");
        const id = Number.parseInt(idStr ?? "", 10);
        const item = catalog.find(
          (c) =>
            (c.source === "did" || c.source === "ford") &&
            c.module_id === moduleId &&
            c.id === id,
        );
        return item ? makeKey(item) : null;
      })
      .filter((k): k is string => k !== null);
    setSelected(new Set(keys));
    setActiveDashboard(dashboard.id);
  };

  const handleStart = () => {
    const pids: number[] = [];
    const dids: LiveDidPin[] = [];

    for (const key of selected) {
      const item = catalog.find((c) => makeKey(c) === key);
      if (!item) continue;
      if (item.source === "pid") {
        pids.push(item.id);
        continue;
      }
      dids.push({
        module_id: item.module_id ?? "",
        did: item.id,
        label: item.label,
        unit: item.unit,
        equation: item.equation,
      });
    }

    onStart(pids, dids);
  };

  const handleRecord = () => {
    if (recording) {
      const rows = [
        ["Timestamp", "Source", "ID", "Module", "Label", "Value", "Unit", "Raw"],
        ...recordedData.map((s) => [
          s.ts_ms.toString(),
          s.source,
          s.id.toString(),
          s.module_id ?? "",
          s.label ?? "",
          s.value?.toString() ?? "",
          s.unit ?? "",
          s.raw_hex,
        ]),
      ];
      downloadCsv(`obd-recording-${Date.now()}.csv`, rows);
      setRecordedData([]);
    }
    setRecording(!recording);
  };

  const filtered = catalog.filter((c) =>
    c.label.toLowerCase().includes(filterText.toLowerCase()),
  );

  const selectedItems = catalog.filter((c) => selected.has(makeKey(c)));

  const chartPoints = (() => {
    if (chartHistory.length < 2) return "";
    const min = Math.min(...chartHistory);
    const max = Math.max(...chartHistory);
    const span = max - min || 1;
    return chartHistory
      .map((v, i) => {
        const x = (i / (chartHistory.length - 1)) * 100;
        const y = 100 - ((v - min) / span) * 100;
        return `${x},${y}`;
      })
      .join(" ");
  })();

  return (
    <section className="flex min-h-0 flex-1 flex-col gap-4">
      <PanelHeader
        title="OBD Live Data"
        subtitle="Select PIDs, DIDs, and Ford extended signals · gauges · CSV"
        actions={
          <>
            <Button variant="ghost" onClick={() => void loadCatalog()}>
              Refresh
            </Button>
            {dashboards.length > 0 && (
              <Select
                value={activeDashboard ?? ""}
                onChange={(id) => {
                  const dash = dashboards.find((d) => d.id === id);
                  if (dash) applyDashboard(dash);
                }}
              >
                <option value="">Load dashboard…</option>
                {dashboards.map((d) => (
                  <option key={`${d.scope}-${d.id}`} value={d.id}>
                    {d.name} ({d.scope})
                  </option>
                ))}
              </Select>
            )}
            <Button
              variant="ghost"
              onClick={() => {
                const name = window.prompt("Dashboard name:");
                if (name) void saveDashboard(name);
              }}
              disabled={selected.size === 0}
            >
              Save dashboard
            </Button>
          </>
        }
      />

      {catalog.length === 0 ? (
        <EmptyState
          title="No signals available"
          description="Connect to the vehicle, then refresh. Pin DIDs from the ECUs tab to add more."
          action={
            <Button variant="primary" onClick={() => void loadCatalog()}>
              Refresh
            </Button>
          }
        />
      ) : (
        <div className="grid min-h-0 flex-1 gap-4 lg:grid-cols-3">
          <div className="flex flex-col gap-3 overflow-auto lg:col-span-1">
            <input
              type="text"
              placeholder="Filter signals…"
              value={filterText}
              onChange={(e) => setFilterText(e.target.value)}
              className="rounded border border-[var(--line)] bg-[var(--bg-elevated)] px-3 py-2 text-sm outline-none focus:border-[var(--accent)]"
            />
            <div className="space-y-1">
              {filtered.map((item) => {
                const key = makeKey(item);
                const isSelected = selected.has(key);
                return (
                  <label
                    key={key}
                    className="flex cursor-pointer items-center gap-2 rounded p-2 hover:bg-[var(--bg-hover)]"
                  >
                    <input
                      type="checkbox"
                      checked={isSelected}
                      onChange={(e) => {
                        const next = new Set(selected);
                        if (e.target.checked) next.add(key);
                        else next.delete(key);
                        setSelected(next);
                      }}
                      className="h-4 w-4"
                    />
                    <div className="flex-1">
                      <p className="text-sm">{item.label}</p>
                      <p className="mono text-xs text-[var(--text-muted)]">
                        {item.source.toUpperCase()} 0x
                        {item.id.toString(16).toUpperCase()}
                        {item.module_id ? ` · ${item.module_id}` : ""}
                        {item.scan_tool && (
                          <span className="ml-2 text-[var(--accent)]">scan</span>
                        )}
                      </p>
                    </div>
                  </label>
                );
              })}
            </div>
          </div>

          <div className="flex flex-col gap-3 overflow-auto lg:col-span-2">
            <div className="flex flex-wrap items-center gap-2">
              {!running ? (
                <Button
                  variant="primary"
                  disabled={selected.size === 0 || !!busy}
                  onClick={handleStart}
                >
                  Start ({selected.size})
                </Button>
              ) : (
                <Button variant="secondary" onClick={onStop}>
                  Stop
                </Button>
              )}
              <Button
                variant={recording ? "danger" : "ghost"}
                onClick={handleRecord}
                disabled={!running && !recording}
              >
                {recording ? "Stop REC" : "REC CSV"}
              </Button>
              {chartSignal && (
                <Button
                  variant="ghost"
                  onClick={() => {
                    setChartSignal(null);
                    setChartHistory([]);
                  }}
                >
                  Close chart
                </Button>
              )}
            </div>

            {chartSignal && (
              <div className="rounded border border-[var(--line)] bg-[var(--bg-elevated)] p-4">
                <p className="mb-2 text-sm text-[var(--text-muted)]">Chart</p>
                <svg viewBox="0 0 100 100" className="h-40 w-full" preserveAspectRatio="none">
                  <polyline
                    fill="none"
                    stroke="var(--accent)"
                    strokeWidth="1.5"
                    points={chartPoints}
                  />
                </svg>
              </div>
            )}

            {selectedItems.length === 0 ? (
              <EmptyState
                title="Nothing selected"
                description="Pick signals from the catalog, then start polling."
              />
            ) : (
              <div className="grid grid-cols-2 gap-3 md:grid-cols-3">
                {selectedItems.map((item) => {
                  const key = makeKey(item);
                  const sample =
                    liveData.get(`pid:${item.id}:`) ??
                    liveData.get(`did:${item.id}:${item.module_id ?? ""}`) ??
                    liveData.get(`pid:${item.id}:${item.module_id ?? ""}`);
                  const display =
                    sample?.value != null
                      ? `${sample.value.toFixed(1)}${sample.unit ? ` ${sample.unit}` : ""}`
                      : (sample?.decoded ?? sample?.raw_hex ?? "—");
                  return (
                    <div
                      key={key}
                      className="rounded border border-[var(--line)] bg-[var(--bg-elevated)] p-4"
                    >
                      <div className="mb-2 flex items-start justify-between gap-2">
                        <p className="text-xs text-[var(--text-muted)]">{item.label}</p>
                        <button
                          type="button"
                          className="text-xs text-[var(--accent)]"
                          onClick={() => {
                            setChartSignal(key);
                            setChartHistory([]);
                          }}
                        >
                          Chart
                        </button>
                      </div>
                      <p className="mono text-2xl font-semibold tracking-tight">{display}</p>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      )}
    </section>
  );
};
