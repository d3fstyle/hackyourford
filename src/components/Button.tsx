import type { ReactNode } from "react";

type ButtonVariant = "primary" | "secondary" | "danger" | "ghost";

interface ButtonProps {
  variant?: ButtonVariant;
  disabled?: boolean;
  onClick?: () => void;
  children: ReactNode;
  className?: string;
  type?: "button" | "submit";
  "aria-label"?: string;
}

const VARIANT_CLASSES: Record<ButtonVariant, string> = {
  primary:
    "bg-[var(--accent)] text-[var(--bg-deep)] font-semibold hover:brightness-110",
  secondary:
    "border border-[var(--line)] hover:bg-[var(--bg-hover)] text-[var(--text)]",
  danger:
    "bg-[var(--danger)] text-white font-semibold hover:brightness-110",
  ghost: "text-[var(--text-muted)] hover:bg-[var(--bg-hover)]",
};

export const Button = ({
  variant = "secondary",
  disabled,
  onClick,
  children,
  className = "",
  type = "button",
  "aria-label": ariaLabel,
}: ButtonProps) => (
  <button
    type={type}
    disabled={disabled}
    onClick={onClick}
    aria-label={ariaLabel}
    className={`rounded px-4 py-2 text-sm transition-colors disabled:opacity-40 disabled:cursor-not-allowed ${VARIANT_CLASSES[variant]} ${className}`}
  >
    {children}
  </button>
);
