type StatusDotVariant = "online" | "offline" | "warning" | "error";

interface StatusDotProps {
  variant: StatusDotVariant;
  label?: string;
}

const VARIANT_COLORS: Record<StatusDotVariant, string> = {
  online: "bg-[var(--accent)]",
  offline: "bg-[var(--text-muted)]",
  warning: "bg-[var(--warn)]",
  error: "bg-[var(--danger)]",
};

export const StatusDot = ({ variant, label }: StatusDotProps) => (
  <span className="flex items-center gap-2">
    <span className={`h-2 w-2 rounded-full ${VARIANT_COLORS[variant]}`} />
    {label && <span className="text-sm">{label}</span>}
  </span>
);
