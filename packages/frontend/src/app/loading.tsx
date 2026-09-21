export default function Loading() {
  return (
    <div
      className="mx-auto flex min-h-[50vh] w-full max-w-6xl items-center justify-center"
      role="status"
      aria-live="polite"
    >
      <div className="flex items-center gap-3 text-sm text-[var(--text-secondary)]">
        <span
          className="h-4 w-4 animate-spin rounded-full border-2 border-[var(--border-strong)] border-t-[var(--accent)]"
          aria-hidden="true"
        />
        Loading LumAgg…
      </div>
    </div>
  );
}
