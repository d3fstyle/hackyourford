import type { ReactNode } from "react";

interface EmptyStateProps {
  title: string;
  description?: string;
  action?: ReactNode;
}

export const EmptyState = ({ title, description, action }: EmptyStateProps) => (
  <div className="flex flex-col items-center justify-center py-16 text-center">
    <h3 className="text-lg font-semibold text-[var(--text-muted)]">{title}</h3>
    {description && (
      <p className="mt-2 max-w-md text-sm text-[var(--text-muted)]">
        {description}
      </p>
    )}
    {action && <div className="mt-4">{action}</div>}
  </div>
);
