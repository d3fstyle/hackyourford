import type { ReactNode } from "react";

interface PanelHeaderProps {
  title: string;
  subtitle?: string;
  actions?: ReactNode;
}

export const PanelHeader = ({ title, subtitle, actions }: PanelHeaderProps) => (
  <div className="flex items-start justify-between gap-4">
    <div>
      <h2 className="text-xl font-semibold">{title}</h2>
      {subtitle && (
        <p className="mt-1 text-sm text-[var(--text-muted)]">{subtitle}</p>
      )}
    </div>
    {actions && <div className="flex items-center gap-2">{actions}</div>}
  </div>
);
