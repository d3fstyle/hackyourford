import { useState } from "react";
import { Button } from "./Button";

interface ExportMenuProps {
  onCopy: () => void;
  onCsv: () => void;
}

export const ExportMenu = ({ onCopy, onCsv }: ExportMenuProps) => {
  const [open, setOpen] = useState(false);

  return (
    <div className="relative">
      <Button variant="ghost" onClick={() => setOpen(!open)}>
        Export ▾
      </Button>
      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} />
          <div className="absolute right-0 top-full z-20 mt-1 w-32 overflow-hidden rounded border border-[var(--line)] bg-[var(--bg-elevated)] shadow-lg">
            <button
              type="button"
              onClick={() => {
                onCopy();
                setOpen(false);
              }}
              className="w-full px-4 py-2 text-left text-sm hover:bg-[var(--bg-hover)]"
            >
              Copy
            </button>
            <button
              type="button"
              onClick={() => {
                onCsv();
                setOpen(false);
              }}
              className="w-full px-4 py-2 text-left text-sm hover:bg-[var(--bg-hover)]"
            >
              CSV
            </button>
          </div>
        </>
      )}
    </div>
  );
};
