import type { ModuleDtcGroup } from "../types";
import { hexId } from "../types";

interface DtcPanelProps {
  groups: ModuleDtcGroup[];
  busy: string | null;
  onReadAll: () => void;
  onClear: (moduleId: string) => void;
}

export const DtcPanel = ({ groups, busy, onReadAll, onClear }: DtcPanelProps) => {
  const total = groups.reduce((n, g) => n + g.dtcs.length, 0);

  return (
    <section className="flex min-h-0 flex-1 flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-xl font-semibold">DTCs</h2>
          <p className="text-sm text-[var(--text-muted)]">
            {total} codes in {groups.filter((g) => g.online).length} modules
          </p>
        </div>
        <button
          type="button"
          onClick={onReadAll}
          disabled={!!busy}
          className="rounded bg-[var(--accent)] px-3 py-1.5 text-sm font-semibold text-[var(--bg-deep)] disabled:opacity-40"
        >
          Read all
        </button>
      </div>

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
                <button
                  type="button"
                  disabled={!!busy}
                  onClick={() => {
                    if (
                      window.confirm(
                        `Clear DTCs for ${g.module_id}? This cannot be undone.`,
                      )
                    ) {
                      onClear(g.module_id);
                    }
                  }}
                  className="rounded border border-[var(--danger)]/40 px-3 py-1 text-xs text-[var(--danger)] hover:bg-[var(--bg-hover)] disabled:opacity-40"
                >
                  Clear DTCs
                </button>
              )}
            </div>
            {g.dtcs.length === 0 && g.online && (
              <p className="text-sm text-[var(--text-muted)]">No DTCs</p>
            )}
            {g.dtcs.length > 0 && (
              <table className="w-full text-left text-sm">
                <thead className="text-xs uppercase text-[var(--text-muted)]">
                  <tr>
                    <th className="py-1">Code</th>
                    <th className="py-1">Status</th>
                    <th className="py-1">Flags</th>
                  </tr>
                </thead>
                <tbody className="mono">
                  {g.dtcs.map((d) => (
                    <tr
                      key={`${d.code}-${d.status}`}
                      className="border-t border-[var(--line)]/60"
                    >
                      <td className="py-1.5">{d.code}</td>
                      <td className="py-1.5 text-[var(--text-muted)]">
                        0x{d.status.toString(16).padStart(2, "0")}
                      </td>
                      <td className="py-1.5 text-xs">
                        {[
                          d.confirmed && "confirmed",
                          d.pending && "pending",
                          d.test_failed && "failed",
                        ]
                          .filter(Boolean)
                          .join(" · ") || "—"}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        ))}
      </div>
    </section>
  );
};
