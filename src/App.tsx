import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Button } from "./components/Button";
import { DtcPanel } from "./features/DtcPanel";
import { EcuPanel } from "./features/EcuPanel";
import { HomePanel } from "./features/HomePanel";
import { LivePanel } from "./features/LivePanel";
import { ProfilesPanel } from "./features/Profiles/ProfilesPanel";
import { ObdPanel } from "./features/Obd/ObdPanel";
import type {
  AppTab,
  AsBuiltBlockWrite,
  AsBuiltDump,
  AsBuiltWriteResult,
  CanFrame,
  ConnectionMode,
  ConnectionStatus,
  DemoFixtureInfo,
  DidValue,
  DiscoveredEcu,
  EcuDiscoverProgress,
  LiveDidPin,
  ModuleDtcGroup,
  ModuleScanResult,
  ParamEdit,
  ParamValue,
  PatternCandidate,
  SerialPortInfo,
  ProfileListItem,
  CaptureStage,
} from "./types";
import { downloadJson } from "./utils/export";

const TABS: { id: AppTab; label: string; needsConn?: boolean }[] = [
  { id: "home", label: "Home" },
  { id: "profiles", label: "Profiles" },
  { id: "ecus", label: "ECUs", needsConn: true },
  { id: "dtcs", label: "DTCs", needsConn: true },
  { id: "live", label: "Live", needsConn: true },
  { id: "obd", label: "OBD", needsConn: true },
];

export default function App() {
  const [ports, setPorts] = useState<SerialPortInfo[]>([]);
  const [fixtures, setFixtures] = useState<DemoFixtureInfo[]>([]);
  const [profileItems, setProfileItems] = useState<ProfileListItem[]>([]);
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
  const [paramValues, setParamValues] = useState<ParamValue[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<AppTab>("home");

  const [liveRunning, setLiveRunning] = useState(false);
  const [pinnedDids, setPinnedDids] = useState<LiveDidPin[]>([]);

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

  const profiles = useMemo(() => profileItems.map((i) => i.profile), [profileItems]);

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

  const refreshProfiles = useCallback(async () => {
    try {
      const items = await invoke<ProfileListItem[]>("list_profiles");
      setProfileItems(items);
      if (items.length > 0 && !profileId) {
        setProfileId(items[0].profile.id);
      }
    } catch (e) {
      setError(String(e));
    }
  }, [profileId]);

  useEffect(() => {
    void (async () => {
      try {
        const [plist, items, fixturesList] = await Promise.all([
          invoke<SerialPortInfo[]>("list_ports"),
          invoke<ProfileListItem[]>("list_profiles"),
          invoke<DemoFixtureInfo[]>("list_demo_fixtures"),
        ]);
        setPorts(plist);
        setProfileItems(items);
        setFixtures(fixturesList);
        if (plist.length > 0) {
          const preferred =
            plist.find((p) => /rfcomm|obd|ttyUSB|ttyACM/i.test(p.name)) ??
            plist[0];
          setPort(preferred.name);
        }
        if (fixturesList.length > 0) setFixtureId(fixturesList[0].id);
        if (items.length > 0) setProfileId(items[0].profile.id);
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
        await listen<string>("connection.lost", (e) => {
          setStatus(null);
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
      setParamValues(null);
      setLiveRunning(false);
      setSniffing(false);
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
      setParamValues(null);
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
      try {
        const params = await invoke<ParamValue[]>("decode_as_built_params", {
          moduleId,
        });
        setParamValues(params);
      } catch {
        setParamValues([]);
      }
      downloadJson(
        `${dump.module_id}-asbuilt-${dump.exported_at_unix_ms}.json`,
        dump,
      );
    });
  }

  async function handleWriteAsBuilt(
    moduleId: string,
    blocks: AsBuiltBlockWrite[],
  ) {
    await withBusy("Writing As-Built…", async () => {
      const result = await invoke<AsBuiltWriteResult>("write_as_built", {
        moduleId,
        blocks,
        fixChecksums: false,
        verify: true,
      });
      if (result.verified && result.verified.length > 0) {
        setAsBuilt({
          module_id: result.module_id,
          module_name: moduleId,
          profile_id: result.profile_id,
          blocks: result.verified,
          exported_at_unix_ms: Date.now(),
        });
      } else {
        const dump = await invoke<AsBuiltDump>("dump_as_built", { moduleId });
        setAsBuilt(dump);
      }
      const params = await invoke<ParamValue[]>("decode_as_built_params", {
        moduleId,
      });
      setParamValues(params);
    });
  }

  async function handleWriteAsBuiltParams(
    moduleId: string,
    edits: ParamEdit[],
  ) {
    await withBusy("Writing parameters…", async () => {
      const result = await invoke<AsBuiltWriteResult>("write_as_built_params", {
        moduleId,
        edits,
        fixChecksums: false,
        verify: true,
      });
      if (result.verified && result.verified.length > 0) {
        setAsBuilt({
          module_id: result.module_id,
          module_name: moduleId,
          profile_id: result.profile_id,
          blocks: result.verified,
          exported_at_unix_ms: Date.now(),
        });
      } else {
        const dump = await invoke<AsBuiltDump>("dump_as_built", { moduleId });
        setAsBuilt(dump);
      }
      const params = await invoke<ParamValue[]>("decode_as_built_params", {
        moduleId,
      });
      setParamValues(params);
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

  async function handleStartLive(pids: number[], dids: LiveDidPin[]) {
    await withBusy("Starting live…", async () => {
      const byKey = new Map<string, LiveDidPin>();
      for (const d of [...dids, ...pinnedDids]) {
        byKey.set(`${d.module_id}:${d.did}`, d);
      }
      await invoke("start_live", {
        pids,
        dids: Array.from(byKey.values()),
        intervalMs: 300,
      });
      setLiveRunning(true);
    });
  }

  async function handleStartSniff(bus: "hs" | "ms") {
    await withBusy("Starting sniff…", async () => {
      setFrames([]);
      setCandidates([]);
      await invoke("start_sniff", { bus });
      setSniffing(true);
    });
  }

  async function handleAnalyzeSequence(stages: CaptureStage[]) {
    return await withBusy("Analyzing sequence…", async () => {
      const c = await invoke<PatternCandidate[]>("analyze_press_sequence", {
        stages,
      });
      setCandidates(c);
      return c;
    }) ?? [];
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
          <Button
            variant="ghost"
            aria-label="Quit"
            className="px-2 leading-none hover:opacity-80"
            onClick={() => void getCurrentWindow().close()}
          >
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              xmlns="http://www.w3.org/2000/svg"
              aria-hidden="true"
            >
              <path
                d="M12 2v10"
                stroke="#ff6214"
                strokeWidth="2.5"
                strokeLinecap="round"
              />
              <path
                d="M6.7 5.6a8 8 0 1 0 10.6 0"
                stroke="#ff6214"
                strokeWidth="2.5"
                strokeLinecap="round"
              />
            </svg>
          </Button>
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
        {tab === "profiles" && (
          <ProfilesPanel
            items={profileItems}
            selectedProfileId={profileId}
            busy={busy}
            onRefresh={() => void refreshProfiles()}
            onSelect={setProfileId}
          />
        )}
        {tab === "ecus" && connected && (
          <EcuPanel
            status={status}
            modules={modules}
            discovered={discovered}
            discoverProgress={discoverProgress}
            busy={busy}
            profile={selectedProfile}
            onScanProfile={() => void handleScan()}
            onDiscover={(b, f, t) => void handleDiscover(b, f, t)}
            onStop={() => void handleStop()}
            onDumpAsBuilt={(id) => void handleDumpAsBuilt(id)}
            onWriteAsBuilt={(id, blocks) => void handleWriteAsBuilt(id, blocks)}
            onWriteAsBuiltParams={(id, edits) =>
              void handleWriteAsBuiltParams(id, edits)
            }
            asBuilt={asBuilt}
            paramValues={paramValues}
            sweptDids={sweptDids}
            didDiffs={didDiffs}
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
            onPinDid={(moduleId, did, label) => {
              setPinnedDids((prev) =>
                prev.some((p) => p.module_id === moduleId && p.did === did)
                  ? prev
                  : [...prev, { module_id: moduleId, did, label }],
              );
            }}
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
            sniffing={sniffing}
            frames={frames}
            candidates={candidates}
            busy={busy}
            onStartSniff={(b) => void handleStartSniff(b)}
            onStop={() => void handleStop()}
            onAnalyzeSequence={handleAnalyzeSequence}
          />
        )}
        {tab === "obd" && connected && (
          <ObdPanel
            running={liveRunning}
            busy={busy}
            profile={selectedProfile}
            pinnedDids={pinnedDids}
            onStart={(pids, dids) => void handleStartLive(pids, dids)}
            onStop={() => void handleStop()}
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
