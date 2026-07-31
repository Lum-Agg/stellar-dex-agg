'use client';

type OrderTypeId = 'instant' | 'limit' | 'dca';

const ORDER_TYPES: {
  id: OrderTypeId;
  label: string;
  hint?: string;
  enabled: boolean;
}[] = [
  { id: 'instant', label: 'Instant', enabled: true },
  { id: 'limit', label: 'Limit', enabled: true },
  { id: 'dca', label: 'DCA', enabled: true },
];

export function OrderTypeRail({
  active = 'instant',
  onSelect,
}: {
  active?: OrderTypeId;
  onSelect?: (id: OrderTypeId) => void;
}) {
  return (
    <aside className="w-full sm:w-[148px] shrink-0">
      <p className="hidden sm:block text-[12px] uppercase tracking-[0.08em] text-[var(--text-muted)] mb-3 px-1">
        Order types
      </p>
      <nav
        className="flex sm:flex-col gap-1 overflow-x-auto sm:overflow-visible pb-1 sm:pb-0"
        aria-label="Order types"
      >
        {ORDER_TYPES.map((item) => {
          const isActive = item.id === active;
          if (!item.enabled) {
            return (
              <div
                key={item.id}
                className="flex items-center justify-between gap-2 rounded-xl px-3 py-2.5 text-[15px] text-[var(--text-muted)]/70 cursor-not-allowed whitespace-nowrap"
                title="Coming soon"
              >
                <span>{item.label}</span>
                {item.hint && (
                  <span className="text-[11px] uppercase tracking-wide text-[var(--text-muted)]/60">
                    {item.hint}
                  </span>
                )}
              </div>
            );
          }
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => {
                onSelect?.(item.id);
              }}
              className={`relative flex items-center rounded-xl px-3 py-2.5 text-[15px] font-medium whitespace-nowrap text-left w-full transition-colors ${
                isActive
                  ? 'bg-[var(--surface-raised)] text-[var(--text-primary)] border border-[var(--border)]'
                  : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] border border-transparent'
              }`}
              aria-current={isActive ? 'page' : undefined}
            >
              {isActive && (
                <span
                  className="absolute left-0 top-1/2 -translate-y-1/2 -translate-x-[1px] hidden sm:block h-5 w-0.5 rounded-full bg-[var(--accent)]"
                  aria-hidden
                />
              )}
              {item.label}
            </button>
          );
        })}
      </nav>
    </aside>
  );
}
