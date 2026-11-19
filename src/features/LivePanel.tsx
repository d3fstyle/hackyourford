import { useEffect, useRef, useState } from "react";
import { Button } from "../components/Button";
import { PanelHeader } from "../components/PanelHeader";
import { Select } from "../components/Select";
import { ExportMenu } from "../components/ExportMenu";
import type { CanFrame, CaptureStage, PatternCandidate } from "../types";
import { hexId } from "../types";
import { downloadCsv, copyText } from "../utils/export";

const PRESS_STAGE_MS = 5_000;
const PRESS_TICK_MS = 50;

const PRESS_STAGE_DEFS: { expectedPresses: number; label: string }[] = [
  { expectedPresses: 0, label: "No pulses. Espera…" },
  { expectedPresses: 1, label: "Pulsa 1 vez y espera" },
  { expectedPresses: 2, label: "Pulsa 2 veces y espera" },
  { expectedPresses: 3, label: "Pulsa 3 veces y espera" },
];

interface LivePanelProps {
  sniffing: boolean;
  frames: CanFrame[];
  candidates: PatternCandidate[];
  busy: string | null;
  onStartSniff: (bus: "hs" | "ms") => void;
  onStop: () => void;
  onAnalyzeSequence: (stages: CaptureStage[]) => Promise<PatternCandidate[]>;
}

export const LivePanel = ({
  sniffing,
  frames,
  candidates,
  busy,
  onStartSniff,
  onStop,
  onAnalyzeSequence,
}: LivePanelProps) => {
  const [bus, setBus] = useState<"hs" | "ms">("ms");
  const [filterText, setFilterText] = useState("");
  const [sequenceOpen, setSequenceOpen] = useState(false);
  const [stageIndex, setStageIndex] = useState(0);
  const [progress, setProgress] = useState(0);
  const [analyzeEmpty, setAnalyzeEmpty] = useState(false);
  const [analyzeResultsOpen, setAnalyzeResultsOpen] = useState(false);
  const [resultCandidates, setResultCandidates] = useState<PatternCandidate[]>(
    [],
  );

  const cancelledRef = useRef(false);
  const stagesRef = useRef<CaptureStage[]>([]);
  const stageStartRef = useRef(0);
  const onAnalyzeSequenceRef = useRef(onAnalyzeSequence);
  onAnalyzeSequenceRef.current = onAnalyzeSequence;

  const filteredFrames = filterText
    ? frames.filter((f) =>
        f.id.toString(16).toLowerCase().includes(filterText.toLowerCase()),
      )
    : frames;

  const handleExportFrames = () => {
    const rows = [
      ["Timestamp", "ID", "Data"],
      ...frames.map((f) => [
        f.ts_ms.toString(),
        hexId(f.id),
        f.data.map((b) => b.toString(16).padStart(2, "0")).join(" ").toUpperCase(),
      ]),
    ];
    downloadCsv(`can-frames-${Date.now()}.csv`, rows);
  };

  const handleCopyFrames = () => {
    const text = frames
      .map(
        (f) =>
          `${f.ts_ms} ${hexId(f.id)} ${f.data.map((b) => b.toString(16).padStart(2, "0")).join(" ").toUpperCase()}`,
      )
      .join("\n");
    void copyText(text);
  };

  const handleExportCandidates = () => {
    const rows = [
      ["CAN ID", "Byte", "Bit", "Score", "Samples"],
      ...candidates.map((c) => [
        hexId(c.can_id),
        c.byte_index.toString(),
        c.bit_index?.toString() ?? "",
        c.change_score.toFixed(2),
        c.sample_values.join(" | "),
      ]),
    ];
    downloadCsv(`candidates-${Date.now()}.csv`, rows);
  };

  const handleCopyCandidates = () => {
    const text = candidates
      .map(
        (c) =>
          `${hexId(c.can_id)} byte ${c.byte_index}${c.bit_index != null ? ` bit ${c.bit_index}` : ""} score ${c.change_score.toFixed(2)}`,
      )
      .join("\n");
    void copyText(text);
  };

  const cancelSequence = () => {
    cancelledRef.current = true;
    setSequenceOpen(false);
    setProgress(0);
    setStageIndex(0);
    stagesRef.current = [];
  };

  const startSequence = () => {
    cancelledRef.current = false;
    stagesRef.current = [];
    setStageIndex(0);
    setProgress(0);
    setAnalyzeEmpty(false);
    setAnalyzeResultsOpen(false);
    setResultCandidates([]);
    stageStartRef.current = Date.now();
    setSequenceOpen(true);
  };

  useEffect(() => {
    if (!sequenceOpen) return;

    const tick = window.setInterval(() => {
      if (cancelledRef.current) {
        window.clearInterval(tick);
        return;
      }

      const elapsed = Date.now() - stageStartRef.current;
      const nextProgress = Math.min(1, elapsed / PRESS_STAGE_MS);
      setProgress(nextProgress);

      if (elapsed < PRESS_STAGE_MS) return;

      window.clearInterval(tick);

      const endMs = Date.now();
      const def = PRESS_STAGE_DEFS[stageIndex];
      stagesRef.current = [
        ...stagesRef.current,
        {
          expected_presses: def.expectedPresses,
          start_ms: stageStartRef.current,
          end_ms: endMs,
        },
      ];

      if (stageIndex + 1 < PRESS_STAGE_DEFS.length) {
        setStageIndex(stageIndex + 1);
        setProgress(0);
        stageStartRef.current = Date.now();
        return;
      }

      setSequenceOpen(false);
      void (async () => {
        if (cancelledRef.current) return;
        const results = await onAnalyzeSequenceRef.current([
          ...stagesRef.current,
        ]);
        if (cancelledRef.current) return;
        setResultCandidates(results);
        if (results.length === 0) {
          setAnalyzeEmpty(true);
          return;
        }
        setAnalyzeResultsOpen(true);
      })();
    }, PRESS_TICK_MS);

    return () => window.clearInterval(tick);
  }, [sequenceOpen, stageIndex]);

  const stageLabel = PRESS_STAGE_DEFS[stageIndex]?.label ?? "";

  return (
    <section className="flex min-h-0 flex-1 flex-col gap-4">
      <PanelHeader
        title="CAN Bus Sniffer"
        subtitle="Monitor CAN frames and analyze press patterns"
      />

      <div className="flex flex-wrap items-center gap-2">
        <Select
          value={bus}
          onChange={(v) => setBus(v as "hs" | "ms")}
          disabled={sniffing}
        >
          <option value="hs">HS-CAN</option>
          <option value="ms">MS-CAN</option>
        </Select>
        {!sniffing ? (
          <Button
            variant="primary"
            disabled={!!busy}
            onClick={() => onStartSniff(bus)}
          >
            Start sniff
          </Button>
        ) : (
          <>
            <Button variant="danger" onClick={onStop} disabled={sequenceOpen}>
              Stop
            </Button>
            <Button
              variant="primary"
              disabled={!!busy || sequenceOpen}
              onClick={startSequence}
            >
              Analyze
            </Button>
          </>
        )}
        <span className="text-xs text-[var(--text-muted)]">
          {frames.length} frames
        </span>
      </div>

      <div className="grid min-h-0 flex-1 gap-4 lg:grid-cols-2">
        <div className="flex min-h-0 flex-col rounded border border-[var(--line)] bg-[var(--bg)]">
          <div className="flex items-center justify-between border-b border-[var(--line)] px-3 py-2">
            <span className="text-xs uppercase text-[var(--text-muted)]">
              Frames ({filteredFrames.length})
            </span>
            <div className="flex items-center gap-2">
              <input
                type="text"
                placeholder="Filter ID..."
                value={filterText}
                onChange={(e) => setFilterText(e.target.value)}
                className="mono w-24 rounded border border-[var(--line)] bg-[var(--bg-elevated)] px-2 py-1 text-xs outline-none focus:border-[var(--accent)]"
              />
              <ExportMenu
                onCopy={handleCopyFrames}
                onCsv={handleExportFrames}
              />
            </div>
          </div>
          <ul className="mono min-h-0 flex-1 overflow-auto p-2 text-xs">
            {[...filteredFrames].slice(-150).reverse().map((f, i) => (
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
          <div className="flex items-center justify-between border-b border-[var(--line)] px-3 py-2">
            <span className="text-xs uppercase text-[var(--text-muted)]">
              Candidates ({candidates.length})
            </span>
            <ExportMenu
              onCopy={handleCopyCandidates}
              onCsv={handleExportCandidates}
            />
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
                Start sniff, then click Analyze for the guided capture.
              </li>
            )}
          </ul>
        </div>
      </div>

      {sequenceOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
          <div
            className="w-full max-w-md rounded-lg border border-[var(--line)] bg-[var(--bg-elevated)] p-6 shadow-xl"
            role="dialog"
            aria-modal="true"
            aria-labelledby="press-sequence-title"
          >
            <h3
              id="press-sequence-title"
              className="text-lg font-semibold"
            >
              Etapa {stageIndex + 1}/{PRESS_STAGE_DEFS.length}
            </h3>
            <p className="mt-3 text-sm text-[var(--text)]">{stageLabel}</p>
            <div className="mt-4 h-2 overflow-hidden rounded bg-[var(--line)]">
              <div
                className="h-full bg-[var(--accent)] transition-[width] duration-75"
                style={{ width: `${Math.round(progress * 100)}%` }}
              />
            </div>
            <div className="mt-6 flex justify-end">
              <Button variant="ghost" onClick={cancelSequence}>
                Cancelar
              </Button>
            </div>
          </div>
        </div>
      )}

      {analyzeEmpty && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
          <div className="w-full max-w-md rounded-lg border border-[var(--line)] bg-[var(--bg-elevated)] p-6 shadow-xl">
            <h3 className="text-lg font-semibold">Analyze</h3>
            <p className="mt-2 text-sm text-[var(--text)]">
              No se detectó ninguna trama relevante
            </p>
            <div className="mt-6 flex justify-end">
              <Button variant="ghost" onClick={() => setAnalyzeEmpty(false)}>
                Cerrar
              </Button>
            </div>
          </div>
        </div>
      )}

      {analyzeResultsOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
          <div className="flex max-h-[80vh] w-full max-w-lg flex-col rounded-lg border border-[var(--line)] bg-[var(--bg-elevated)] p-6 shadow-xl">
            <h3 className="text-lg font-semibold">
              Candidatos ({resultCandidates.length})
            </h3>
            <p className="mt-2 text-xs text-[var(--text-muted)]">
              Resultados del análisis guiado. También aparecen en el panel
              Candidates.
            </p>
            <ul className="mt-4 min-h-0 flex-1 overflow-auto text-sm">
              {resultCandidates.map((c) => (
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
            </ul>
            <div className="mt-6 flex justify-end">
              <Button
                variant="ghost"
                onClick={() => setAnalyzeResultsOpen(false)}
              >
                Cerrar
              </Button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
};
