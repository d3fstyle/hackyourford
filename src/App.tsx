import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { DiscoverPanel } from "./features/DiscoverPanel";
import { DtcPanel } from "./features/DtcPanel";
import { EcuPanel } from "./features/EcuPanel";
import { HomePanel } from "./features/HomePanel";
import { LivePanel } from "./features/LivePanel";
import type {
  AppTab,
  AsBuiltDump,
  CanFrame,
  ConnectionMode,
  ConnectionStatus,
  DemoFixtureInfo,
  DidValue,
  DiscoveredEcu,
  EcuDiscoverProgress,
  LiveSample,
  ModuleDtcGroup,
  ModuleScanResult,
  PatternCandidate,
  SerialPortInfo,
  VehicleProfile,
} from "./types";
import { downloadJson } from "./types";

const TABS: { id: AppTab; label: string; needsConn?: boolean }[] = [
  { id: "home", label: "Home" },
  { id: "ecus", label: "ECUs", needsConn: true },
  { id: "dtcs", label: "DTCs", needsConn: true },
  { id: "live", label: "Live", needsConn: true },
  { id: "discover", label: "Discover", needsConn: true },
];

export default function App() {
  const [ports, setPorts] = useState<SerialPortInfo[]>([]);
  const [fixtures, setFixtures] = useState<DemoFixtureInfo[]>([]);
  const [profiles, setProfiles] = useState<VehicleProfile[]>([]);
  const [connectionMode, setConnectionMode] = useState<ConnectionMode>("serial");
  const [port, setPort] = useState("");
  const [fixtureId, setFixtureId] = useState("");
  const [profileId, setProfileId] = useState("");
  const [status, setStatus] = useState<ConnectionStatus | null>(null);
  const [modules, setModules] = useState<ModuleScanResult[]>([]);
  const [discovered, setDiscovered] = useState<DiscoveredEcu[]>([]);
  const [discoverProgress, setDiscoverProgress] =
    useState<EcuDiscoverProgress | null>(null);
  const [dtcGroups, setDtcGroups] = useState<ModuleDtcGroup[]>([]);
  const [asBuilt, setAsBuilt] = useState<AsBuiltDump | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<AppTab>("home");

  const [liveRunning, setLiveRunning] = useState(false);
  const [liveStream, setLiveStream] = useState<LiveSample[]>([]);
  const [foundDids, setFoundDids] = useState<LiveSample[]>([]);
  const [foundPids, setFoundPids] = useState<LiveSample[]>([]);
  const [pinnedDids, setPinnedDids] = useState<
    [string, number, string | null][]
  >([]);

  const [sniffing, setSniffing] = useState(false);
  const [frames, setFrames] = useState<CanFrame[]>([]);
  const [candidates, setCandidates] = useState<PatternCandidate[]>([]);
  const [sweptDids, setSweptDids] = useState<DidValue[]>([]);
  const [didDiffs, setDidDiffs] = useState<[number, string, string][]>([]);
  const [mcpBridge, setMcpBridge] = useState<{
    port: number;
    token_path: string;
  } | null>(null);

  const connected = status !== null;
  const isDemo = status?.port.startsWith("demo:") ?? false;

  const selectedProfile = useMemo(
    () => profiles.find((p) => p.id === profileId) ?? null,
    [profiles, profileId],
  );

  const refreshPorts = useCallback(async () => {
    try {
      const list = await invoke<SerialPortInfo[]>("list_ports");
      setPorts(list);
      if (!port && list.length > 0) {
        const preferred =
          list.find((p) => /rfcomm|obd|ttyUSB|ttyACM/i.test(p.name)) ?? list[0];
        setPort(preferred.name);
      }
    } catch (e) {
      setError(String(e));
    }
  }, [port]);

  useEffect(() => {
    void (async () => {
      try {
        const [plist, profilesList, fixturesList] = await Promise.all([
          invoke<SerialPortInfo[]>("list_ports"),
          invoke<VehicleProfile[]>("list_profiles"),
          invoke<DemoFixtureInfo[]>("list_demo_fixtures"),
        ]);
        setPorts(plist);
        setProfiles(profilesList);
        setFixtures(fixturesList);
        if (plist.length > 0) {
          const preferred =
            plist.find((p) => /rfcomm|obd|ttyUSB|ttyACM/i.test(p.name)) ??
            plist[0];
          setPort(preferred.name);
        }
        if (fixturesList.length > 0) setFixtureId(fixturesList[0].id);
        if (profilesList.length > 0) setProfileId(profilesList[0].id);
      } catch (e) {
        setError(String(e));
      }
    })();
  }, []);

  useEffect(() => {
    const unsubs: (() => void)[] = [];
    void (async () => {
      unsubs.push(
        await listen<EcuDiscoverProgress>("ecu.progress", (e) => {
          setDiscoverProgress(e.payload);
        }),
      );
      unsubs.push(
        await listen<DiscoveredEcu[]>("ecu.done", (e) => {
          setDiscovered(e.payload);
          setDiscoverProgress(null);
        }),
      );
      unsubs.push(
        await listen<LiveSample>("live.sample", (e) => {
          setLiveStream((prev) => [...prev.slice(-500), e.payload]);
        }),
      );
      unsubs.push(
        await listen<LiveSample>("live.did_found", (e) => {
          setFoundDids((prev) =>
            prev.some((x) => x.id === e.payload.id)
              ? prev.map((x) => (x.id === e.payload.id ? e.payload : x))
              : [...prev, e.payload],
          );
        }),
      );
      unsubs.push(
        await listen<LiveSample>("live.pid_found", (e) => {
          setFoundPids((prev) =>
            prev.some((x) => x.id === e.payload.id)
              ? prev.map((x) => (x.id === e.payload.id ? e.payload : x))
              : [...prev, e.payload],
          );
        }),
      );
      unsubs.push(
        await listen<CanFrame>("sniff.frame", (e) => {
          setFrames((prev) => [...prev.slice(-2000), e.payload]);
        }),
      );
      unsubs.push(
        await listen<PatternCandidate[]>("sniff.candidates", (e) => {
          setCandidates(e.payload);
        }),
      );
      unsubs.push(
        await listen<DidValue>("did.found", (e) => {
          setSweptDids((prev) =>
            prev.some((d) => d.did === e.payload.did)
              ? prev
              : [...prev, e.payload],
          );
        }),
      );
      unsubs.push(
        await listen<string>("worker.mode", (e) => {
          const m = String(e.payload).toLowerCase();
          setLiveRunning(m === "live");
          setSniffing(m === "can_sniff");
        }),
      );
      unsubs.push(
        await listen<string>("worker.error", (e) => {
          setError(String(e.payload));
        }),
      );
      unsubs.push(
        await listen<{ port: number; token_path: string }>(
          "agent.bridge",
          (e) => {
            setMcpBridge(e.payload);
          },
        ),
      );
    })();
    return () => unsubs.forEach((u) => u());
  }, []);

  async function withBusy<T>(
    label: string,
    fn: () => Promise<T>,
  ): Promise<T | undefined> {
    setBusy(label);
    setError(null);
    try {
      return await fn();
    } catch (e) {
      setError(String(e));
      return undefined;
    } finally {
      setBusy(null);
    }
  }

  async function handleConnect() {
    await withBusy("Connecting…", async () => {
      let s: ConnectionStatus;
      let nextProfileId = profileId;
      if (connectionMode === "demo") {
        const fixture = fixtures.find((f) => f.id === fixtureId);
        if (fixture?.profile_hint) {
          nextProfileId = fixture.profile_hint;
          setProfileId(fixture.profile_hint);
        }
        s = await invoke<ConnectionStatus>("connect_demo", { fixtureId });
      } else {
        s = await invoke<ConnectionStatus>("connect", { port, baud: 115200 });
      }
      setStatus(s);
      if (nextProfileId) {
        const updated = await invoke<ConnectionStatus>("select_profile", {
          profileId: nextProfileId,
        });
        setStatus(updated);
      }
      const vin = await invoke<string | null>("read_vin");
      setStatus((prev) => (prev ? { ...prev, vin } : prev));
    });
  }

  async function handleDisconnect() {
    await withBusy("Disconnecting…", async () => {
      await invoke("stop_activity");
      await invoke("disconnect");
      setStatus(null);
      setModules([]);
      setDiscovered([]);
      setDtcGroups([]);
      setAsBuilt(null);
      setLiveRunning(false);
      setSniffing(false);
      setLiveStream([]);
      setFoundDids([]);
      setFoundPids([]);
      setFrames([]);
      setTab("home");
    });
  }

  async function handleScan() {
    await withBusy("Scanning modules…", async () => {
      if (profileId) {
        await invoke("select_profile", { profileId });
      }
      const results = await invoke<ModuleScanResult[]>("scan_modules");
      setModules(results);
      setTab("ecus");
      setAsBuilt(null);
      const s = await invoke<ConnectionStatus | null>("connection_status");
      if (s) setStatus(s);
    });
  }

  async function handleDiscover(bus: "hs" | "ms", fromId: number, toId: number) {
    await withBusy("Discovering ECUs…", async () => {
      const found = await invoke<DiscoveredEcu[]>("discover_ecus", {
        bus,
        fromId,
        toId,
      });
      setDiscovered(found);
      setDiscoverProgress(null);
    });
  }

  async function handleDumpAsBuilt(moduleId: string) {
    await withBusy("Dumping As-Built…", async () => {
      const dump = await invoke<AsBuiltDump>("dump_as_built", { moduleId });
      setAsBuilt(dump);
      downloadJson(
        `${dump.module_id}-asbuilt-${dump.exported_at_unix_ms}.json`,
        dump,
      );
    });
  }

  async function handleReadAllDtcs() {
    await withBusy("Reading all DTCs…", async () => {
      const groups = await invoke<ModuleDtcGroup[]>("read_all_dtcs");
      setDtcGroups(groups);
    });
  }

  async function handleClearDtcs(moduleId: string) {
    await withBusy("Clearing DTCs…", async () => {
      await invoke("clear_dtcs", { moduleId });
      const groups = await invoke<ModuleDtcGroup[]>("read_all_dtcs");
      setDtcGroups(groups);
    });
  }

  async function handleStop() {
    await withBusy("Stopping…", async () => {
      await invoke("stop_activity");
      setLiveRunning(false);
      setSniffing(false);
      setDiscoverProgress(null);
    });
  }

  async function handleStartLive(
    pids: number[],
    dids: [string, number, string | null][],
  ) {
    await withBusy("Starting live…", async () => {
      setLiveStream([]);
      setFoundDids([]);
      setFoundPids([]);
      const merged = [...dids, ...pinnedDids];
      await invoke("start_live", {
        pids,
        dids: merged,
        intervalMs: 300,
      });
      setLiveRunning(true);
    });
  }

  async function handleSaveLive() {
    try {
      const path = await save({
        defaultPath: `forddiag-live-${Date.now()}.json`,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return;
      await withBusy("Saving…", async () => {
        await invoke("save_live_session", { path });
      });
    } catch (e) {
      // Fallback: download via browser
      const samples = await invoke<LiveSample[]>("get_live_samples");
      downloadJson(`forddiag-live-${Date.now()}.json`, { samples });
      setError(String(e));
    }
  }

  async function handleStartSniff(bus: "hs" | "ms") {
    await withBusy("Starting sniff…", async () => {
      setFrames([]);
      setCandidates([]);
      await invoke("start_sniff", { bus });
      setSniffing(true);
    });
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex items-center justify-between border-b border-[var(--line)] px-8 py-4">
        <div className="flex items-baseline gap-3">
          <h1 className="text-2xl font-bold tracking-tight text-[var(--accent)]">
            FordDiag
          </h1>
          <span className="text-sm text-[var(--text-muted)]">Linux · OBD</span>
        </div>
        <div className="flex items-center gap-3 text-sm">
          {status && (
            <span className="mono text-[var(--text-muted)]">
              {status.adapter.identity}
              {status.adapter.voltage != null &&
                ` · ${status.adapter.voltage.toFixed(1)}V`}
            </span>
          )}
          {isDemo && (
            <span className="rounded px-2 py-0.5 text-xs font-medium bg-[var(--warn)]/20 text-[var(--warn)]">
              DEMO
            </span>
          )}
          <span
            className={`rounded px-2 py-0.5 text-xs font-medium ${
              connected
                ? "bg-[var(--accent-dim)]/30 text-[var(--accent)]"
                : "bg-[var(--bg-elevated)] text-[var(--text-muted)]"
            }`}
          >
            {connected ? "Connected" : "Disconnected"}
          </span>
        </div>
      </header>

      <nav className="flex gap-1 border-b border-[var(--line)] px-8 pt-2">
        {TABS.map((t) => {
          const disabled = !!t.needsConn && !connected;
          return (
            <button
              key={t.id}
              type="button"
              disabled={disabled}
              onClick={() => setTab(t.id)}
              className={`rounded-t px-4 py-2 text-sm ${
                tab === t.id
                  ? "bg-[var(--bg)] font-semibold text-[var(--accent)]"
                  : "text-[var(--text-muted)] hover:bg-[var(--bg-hover)]"
              } disabled:opacity-30`}
            >
              {t.label}
            </button>
          );
        })}
      </nav>

      <main className="flex min-h-0 flex-1 flex-col gap-6 overflow-auto px-8 py-6">
        {tab === "home" && (
          <HomePanel
            connectionMode={connectionMode}
            setConnectionMode={setConnectionMode}
            connected={connected}
            busy={busy}
            ports={ports}
            port={port}
            setPort={setPort}
            fixtures={fixtures}
            fixtureId={fixtureId}
            setFixtureId={setFixtureId}
            profiles={profiles}
            profileId={profileId}
            setProfileId={setProfileId}
            selectedProfile={selectedProfile}
            status={status}
            onRefreshPorts={() => void refreshPorts()}
            onConnect={() => void handleConnect()}
            onDisconnect={() => void handleDisconnect()}
            onScan={() => void handleScan()}
          />
        )}
        {tab === "ecus" && connected && (
          <EcuPanel
            status={status}
            modules={modules}
            discovered={discovered}
            discoverProgress={discoverProgress}
            busy={busy}
            onScanProfile={() => void handleScan()}
            onDiscover={(b, f, t) => void handleDiscover(b, f, t)}
            onStop={() => void handleStop()}
            onDumpAsBuilt={(id) => void handleDumpAsBuilt(id)}
            asBuilt={asBuilt}
          />
        )}
        {tab === "dtcs" && connected && (
          <DtcPanel
            groups={dtcGroups}
            busy={busy}
            onReadAll={() => void handleReadAllDtcs()}
            onClear={(id) => void handleClearDtcs(id)}
          />
        )}
        {tab === "live" && connected && (
          <LivePanel
            running={liveRunning}
            busy={busy}
            stream={liveStream}
            foundDids={foundDids}
            foundPids={foundPids}
            profile={selectedProfile}
            onStart={(pids, dids) => void handleStartLive(pids, dids)}
            onStop={() => void handleStop()}
            onSave={() => void handleSaveLive()}
            onProbePids={() => invoke<number[]>("list_supported_pids")}
          />
        )}
        {tab === "discover" && connected && (
          <DiscoverPanel
            profile={selectedProfile}
            busy={busy}
            sniffing={sniffing}
            frames={frames}
            candidates={candidates}
            sweptDids={sweptDids}
            didDiffs={didDiffs}
            onStartSniff={(b) => void handleStartSniff(b)}
            onStop={() => void handleStop()}
            onMarkPress={() => void invoke("mark_press")}
            onAnalyze={() =>
              void withBusy("Analyzing…", async () => {
                const c = await invoke<PatternCandidate[]>("analyze_pattern");
                setCandidates(c);
              })
            }
            onDidSweep={(moduleId, from, to) =>
              void withBusy("DID sweep…", async () => {
                setSweptDids([]);
                const found = await invoke<DidValue[]>("start_did_sweep", {
                  moduleId,
                  fromDid: from,
                  toDid: to,
                });
                setSweptDids(found);
              })
            }
            onSnapshot={(moduleId, from, to, which) =>
              void withBusy(`Snapshot ${which}…`, async () => {
                await invoke("did_snapshot", {
                  moduleId,
                  fromDid: from,
                  toDid: to,
                  which,
                });
              })
            }
            onDiff={() =>
              void withBusy("Diff…", async () => {
                const diffs = await invoke<[number, string, string][]>(
                  "diff_did_snapshots",
                );
                setDidDiffs(diffs);
              })
            }
            onPinDid={(moduleId, did) => {
              setPinnedDids((prev) =>
                prev.some((p) => p[0] === moduleId && p[1] === did)
                  ? prev
                  : [...prev, [moduleId, did, null]],
              );
            }}
          />
        )}
      </main>

      <footer className="flex items-center justify-between gap-4 border-t border-[var(--line)] px-8 py-2 text-xs text-[var(--text-muted)]">
        <span>{busy ?? "Ready"}</span>
        <span className="flex items-center gap-3">
          {mcpBridge && (
            <span className="mono text-[var(--accent)]" title={mcpBridge.token_path}>
              MCP :{mcpBridge.port}
            </span>
          )}
          {error && <span className="text-[var(--danger)]">{error}</span>}
        </span>
      </footer>
    </div>
  );
}
