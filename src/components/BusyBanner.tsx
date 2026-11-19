interface BusyBannerProps {
  message: string;
}

export const BusyBanner = ({ message }: BusyBannerProps) => (
  <div className="flex items-center gap-3 rounded border border-[var(--accent)]/30 bg-[var(--accent)]/10 px-4 py-3">
    <div className="h-4 w-4 animate-spin rounded-full border-2 border-[var(--accent)] border-t-transparent" />
    <span className="text-sm text-[var(--accent)]">{message}</span>
  </div>
);
