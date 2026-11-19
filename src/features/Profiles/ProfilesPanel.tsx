import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "../../components/Button";
import { PanelHeader } from "../../components/PanelHeader";
import { EmptyState } from "../../components/EmptyState";
import { ConfirmDialog } from "../../components/ConfirmDialog";
import type { ProfileListItem, VehicleProfile, ModuleDef, ParsedAsBuilt } from "../../types";

interface ProfilesPanelProps {
  items: ProfileListItem[];
  selectedProfileId: string | null;
  busy: string | null;
  onRefresh: () => void;
  onSelect: (profileId: string) => void;
}

export const ProfilesPanel = ({
  items,
  selectedProfileId,
  busy,
  onRefresh,
  onSelect,
}: ProfilesPanelProps) => {
  const [editing, setEditing] = useState<string | null>(null);
  const [editName, setEditName] = useState("");
  const [editYears, setEditYears] = useState("");
  const [editNotes, setEditNotes] = useState("");
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const selectedItem = items.find((i) => i.profile.id === selectedProfileId);

  const handleEdit = (item: ProfileListItem) => {
    setEditing(item.profile.id);
    setEditName(item.profile.name);
    setEditYears(item.profile.years);
    setEditNotes(item.profile.notes ?? "");
  };

  const handleSave = async () => {
    if (!editing) return;
    try {
      const profile = items.find((i) => i.profile.id === editing)?.profile;
      if (!profile) return;
      await invoke("save_profile", {
        profile: {
          ...profile,
          name: editName,
          years: editYears,
          notes: editNotes || null,
        },
      });
      setEditing(null);
      setError(null);
      setMessage("Profile saved");
      onRefresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleNew = async () => {
    try {
      const id = `user_${Date.now()}`;
      const newProfile = await invoke<VehicleProfile>("create_blank_profile", {
        id,
        name: "New Profile",
        years: "",
      });
      await invoke("save_profile", { profile: newProfile });
      setError(null);
      setMessage("Created new profile");
      onRefresh();
      onSelect(newProfile.id);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDelete = async () => {
    if (!deleteConfirm) return;
    try {
      await invoke("delete_profile", { profileId: deleteConfirm });
      setDeleteConfirm(null);
      setError(null);
      setMessage("Profile deleted");
      onRefresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleImport = async () => {
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "As-Built XML", extensions: ["ab", "xml"] }],
      });
      if (!path || typeof path !== "string") return;

      const content = await invoke<string>("read_text_file", { path });
      const parsed = await invoke<ParsedAsBuilt>("import_as_built_xml", {
        xmlContent: content,
      });
      const summary = await invoke<string>("as_built_import_summary", { parsed });

      const base =
        selectedItem?.profile ??
        (await invoke<VehicleProfile>("create_blank_profile", {
          id: `ab_${Date.now()}`,
          name: parsed.vin ? `As-Built ${parsed.vin}` : "Imported As-Built",
          years: "",
        }));

      const merged = await invoke<VehicleProfile>("apply_as_built_to_profile", {
        profile: base,
        parsed,
      });
      await invoke("save_profile", { profile: merged });
      setError(null);
      setMessage(summary);
      onRefresh();
      onSelect(merged.id);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <section className="flex min-h-0 flex-1 flex-col gap-4">
      <PanelHeader
        title="Vehicle Profiles"
        subtitle="Manage profiles and import dealer As-Built (.ab) files"
        actions={
          <>
            <Button variant="ghost" onClick={() => void handleNew()} disabled={!!busy}>
              New
            </Button>
            <Button variant="ghost" onClick={() => void handleImport()} disabled={!!busy}>
              Import .ab
            </Button>
          </>
        }
      />

      {error && (
        <div className="rounded border border-[var(--danger)]/30 bg-[var(--danger)]/10 px-4 py-3 text-sm text-[var(--danger)]">
          {error}
        </div>
      )}
      {message && !error && (
        <div className="rounded border border-[var(--accent)]/30 bg-[var(--accent)]/10 px-4 py-3 text-sm text-[var(--accent)]">
          {message}
        </div>
      )}

      {items.length === 0 ? (
        <EmptyState
          title="No profiles"
          description="Create a new profile or import As-Built data"
          action={
            <Button variant="primary" onClick={() => void handleNew()}>
              New profile
            </Button>
          }
        />
      ) : (
        <div className="grid min-h-0 flex-1 gap-4 lg:grid-cols-2">
          <div className="flex flex-col gap-2 overflow-auto">
            {items.map((item) => {
              const isEditing = editing === item.profile.id;
              const isSelected = selectedProfileId === item.profile.id;

              if (isEditing) {
                return (
                  <div
                    key={item.profile.id}
                    className="rounded border border-[var(--accent)] bg-[var(--bg-elevated)] p-4"
                  >
                    <label className="mb-2 block">
                      <span className="text-xs text-[var(--text-muted)]">Name</span>
                      <input
                        type="text"
                        value={editName}
                        onChange={(e) => setEditName(e.target.value)}
                        className="mt-1 w-full rounded border border-[var(--line)] bg-[var(--bg)] px-3 py-2 text-sm outline-none focus:border-[var(--accent)]"
                      />
                    </label>
                    <label className="mb-2 block">
                      <span className="text-xs text-[var(--text-muted)]">Years</span>
                      <input
                        type="text"
                        value={editYears}
                        onChange={(e) => setEditYears(e.target.value)}
                        className="mt-1 w-full rounded border border-[var(--line)] bg-[var(--bg)] px-3 py-2 text-sm outline-none focus:border-[var(--accent)]"
                      />
                    </label>
                    <label className="mb-4 block">
                      <span className="text-xs text-[var(--text-muted)]">Notes</span>
                      <textarea
                        value={editNotes}
                        onChange={(e) => setEditNotes(e.target.value)}
                        rows={3}
                        className="mt-1 w-full rounded border border-[var(--line)] bg-[var(--bg)] px-3 py-2 text-sm outline-none focus:border-[var(--accent)]"
                      />
                    </label>
                    <div className="flex gap-2">
                      <Button variant="primary" onClick={() => void handleSave()}>
                        Save
                      </Button>
                      <Button variant="ghost" onClick={() => setEditing(null)}>
                        Cancel
                      </Button>
                    </div>
                  </div>
                );
              }

              return (
                <div
                  key={item.profile.id}
                  role="button"
                  tabIndex={0}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") onSelect(item.profile.id);
                  }}
                  className={`cursor-pointer rounded border p-4 transition-colors ${
                    isSelected
                      ? "border-[var(--accent)] bg-[var(--bg-elevated)]"
                      : "border-[var(--line)] bg-[var(--bg-elevated)] hover:border-[var(--accent-dim)]"
                  }`}
                  onClick={() => onSelect(item.profile.id)}
                >
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex-1">
                      <h3 className="font-semibold">{item.profile.name}</h3>
                      <p className="text-sm text-[var(--text-muted)]">{item.profile.years}</p>
                      {item.profile.notes && (
                        <p className="mt-1 text-xs text-[var(--text-muted)]">
                          {item.profile.notes}
                        </p>
                      )}
                      <p className="mt-2 text-xs text-[var(--text-muted)]">
                        {item.profile.modules.length} modules
                        {item.is_builtin && !item.can_delete && " · Built-in"}
                        {item.is_builtin && item.can_delete && " · Built-in · edited"}
                      </p>
                    </div>
                    <div className="flex gap-1">
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          handleEdit(item);
                        }}
                        className="rounded px-2 py-1 text-xs text-[var(--text-muted)] hover:bg-[var(--bg-hover)]"
                      >
                        Edit
                      </button>
                      {item.can_delete && (
                        <button
                          type="button"
                          onClick={(e) => {
                            e.stopPropagation();
                            setDeleteConfirm(item.profile.id);
                          }}
                          className="rounded px-2 py-1 text-xs text-[var(--danger)] hover:bg-[var(--danger)]/10"
                        >
                          Delete
                        </button>
                      )}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>

          {selectedItem && (
            <div className="overflow-auto rounded border border-[var(--line)] bg-[var(--bg-elevated)] p-4">
              <h3 className="mb-4 font-semibold">Modules</h3>
              <div className="space-y-2">
                {selectedItem.profile.modules.map((mod) => (
                  <ModuleCard key={mod.id} module={mod} />
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      <ConfirmDialog
        open={deleteConfirm !== null}
        title="Delete profile"
        message="Are you sure you want to delete this profile? This cannot be undone."
        confirmLabel="Delete"
        danger
        onConfirm={() => void handleDelete()}
        onCancel={() => setDeleteConfirm(null)}
      />
    </section>
  );
};

const ModuleCard = ({ module }: { module: ModuleDef }) => (
  <div className="rounded border border-[var(--line)] bg-[var(--bg)] p-3">
    <div className="flex items-start justify-between">
      <div className="flex-1">
        <h4 className="text-sm font-semibold">{module.name}</h4>
        <p className="mono mt-1 text-xs text-[var(--text-muted)]">
          ID: {module.id} · TX: 0x{module.tx_id.toString(16).toUpperCase()} ·{" "}
          {module.bus.toUpperCase()}-CAN
        </p>
      </div>
    </div>
    <p className="mt-2 text-xs text-[var(--text-muted)]">
      {module.dids.length} DIDs
      {module.params && module.params.length > 0
        ? ` · ${module.params.length} params`
        : ""}
    </p>
  </div>
);
