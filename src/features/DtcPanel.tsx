import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ModuleDtcGroup, DtcInfo } from "../types";
import { hexId } from "../types";
import { Button } from "../components/Button";
import { PanelHeader } from "../components/PanelHeader";
import { ExportMenu } from "../components/ExportMenu";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { downloadCsv, copyText } from "../utils/export";

interface DtcPanelProps {
  groups: ModuleDtcGroup[];
  busy: string | null;
  onReadAll: () => void;
  onClear: (moduleId: string) => void;
}

export const DtcPanel = ({ groups, busy, onReadAll, onClear }: DtcPanelProps) => {
  const total = groups.reduce((n, g) => n + g.dtcs.length, 0);
  const [dtcInfo, setDtcInfo] = useState<Map<string, DtcInfo>>(new Map());
  const [clearConfirm, setClearConfirm] = useState<string | null>(null);

  useEffect(() => {
    const codes = groups.flatMap((g) => g.dtcs.map((d) => d.code));
    void loadDtcInfo(codes);
  }, [groups]);

  const loadDtcInfo = async (codes: string[]) => {
    const map = new Map<string, DtcInfo>();
    for (const code of codes) {
      if (dtcInfo.has(code)) continue;
      try {
        const info = await invoke<DtcInfo | null>("lookup_dtc", { code });
        if (info) map.set(code, info);
      } catch {
        // lookup_dtc may not be available or code not found
      }
    }
    setDtcInfo((prev) => new Map([...prev, ...map]));
  };

  const handleExport = () => {
    const rows = [
      ["Module", "Code", "Status", "Flags", "Name", "Description"],
      ...groups.flatMap((g) =>
        g.dtcs.map((d) => {
          const info = dtcInfo.get(d.code);
          const flags = [
            d.confirmed && "confirmed",
            d.pending && "pending",
            d.test_failed && "failed",
          ]
            .filter(Boolean)
            .join(" · ") || "—";
          return [
            g.module_name,
            d.code,
            `0x${d.status.toString(16).padStart(2, "0")}`,
            flags,
            info?.name ?? "",
            info?.description ?? "",
          ];
        }),
      ),
    ];
    downloadCsv(`dtcs-${Date.now()}.csv`, rows);
  };

  const handleCopy = () => {
    const text = groups
      .flatMap((g) =>
        g.dtcs.map((d) => {
          const info = dtcInfo.get(d.code);
          return `${g.module_name}: ${d.code}${info ? ` - ${info.name}` : ""}`;
        }),
      )
      .join("\n");
    void copyText(text);
  };

  return (
    <section className="flex min-h-0 flex-1 flex-col gap-4">
      <PanelHeader
        title="DTCs"
        subtitle={`${total} codes in ${groups.filter((g) => g.online).length} modules`}
        actions={
          <>
            <ExportMenu onCopy={handleCopy} onCsv={handleExport} />
            <Button variant="primary" onClick={onReadAll} disabled={!!busy}>
              Read all
            </Button>
          </>
        }
      />

      <div className="min-h-0 flex-1 space-y-4 overflow-auto">
        {groups.length === 0 && (
          <p className="text-sm text-[var(--text-muted)]">
            Press “Read all” to scan DTCs on each profile module.
          </p>
        )}
        {groups.map((g) => (
          <div
            key={g.module_id}
            className="rounded border border-[var(--line)] bg-[var(--bg)] p-4"
          >
            <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
              <div>
                <h3 className="font-semibold">
                  {g.module_name}{" "}
                  <span className="mono text-sm text-[var(--text-muted)]">
                    ({g.module_id} · {hexId(g.tx_id)})
                  </span>
                </h3>
                {!g.online && g.error && (
                  <p className="text-sm text-[var(--warn)]">{g.error}</p>
                )}
              </div>
              {g.online && (
                <Button
                  variant="danger"
                  disabled={!!busy}
                  onClick={() => setClearConfirm(g.module_id)}
                >
                  Clear DTCs
                </Button>
              )}
            </div>
            {g.dtcs.length === 0 && g.online && (
              <p className="text-sm text-[var(--text-muted)]">No DTCs</p>
            )}
            {g.dtcs.length > 0 && (
              <div className="space-y-2">
                {g.dtcs.map((d) => {
                  const info = dtcInfo.get(d.code);
                  const flags = [
                    d.confirmed && "confirmed",
                    d.pending && "pending",
                    d.test_failed && "failed",
                  ]
                    .filter(Boolean)
                    .join(" · ") || "—";
                  return (
                    <div
                      key={`${d.code}-${d.status}`}
                      className="rounded border border-[var(--line)]/60 bg-[var(--bg-elevated)] p-3"
                    >
                      <div className="flex items-start justify-between gap-2">
                        <div className="flex-1">
                          <div className="flex items-baseline gap-2">
                            <span className="mono font-semibold">{d.code}</span>
                            <span className="mono text-xs text-[var(--text-muted)]">
                              0x{d.status.toString(16).padStart(2, "0")}
                            </span>
                          </div>
                          {info && (
                            <>
                              <p className="mt-1 text-sm font-medium">{info.name}</p>
                              <p className="mt-1 text-xs text-[var(--text-muted)]">
                                {info.description}
                              </p>
                            </>
                          )}
                        </div>
                        <span className="whitespace-nowrap text-xs text-[var(--text-muted)]">
                          {flags}
                        </span>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        ))}
      </div>

      <ConfirmDialog
        open={clearConfirm !== null}
        title="Clear DTCs"
        message={`Are you sure you want to clear DTCs for ${clearConfirm}? This cannot be undone.`}
        confirmLabel="Clear"
        danger
        onConfirm={() => {
          if (clearConfirm) onClear(clearConfirm);
          setClearConfirm(null);
        }}
        onCancel={() => setClearConfirm(null)}
      />
    </section>
  );
};
