import type { ReactNode } from "react";

interface SelectProps {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  children: ReactNode;
  className?: string;
}

export const Select = ({
  value,
  onChange,
  disabled,
  children,
  className = "",
}: SelectProps) => (
  <select
    value={value}
    onChange={(e) => onChange(e.target.value)}
    disabled={disabled}
    className={`rounded border border-[var(--accent-dim)] bg-[color-mix(in_srgb,var(--accent-dim)_30%,var(--bg-elevated))] px-3 py-2 text-sm text-[var(--text)] outline-none focus:border-[var(--accent)] disabled:opacity-40 ${className}`}
  >
    {children}
  </select>
);
