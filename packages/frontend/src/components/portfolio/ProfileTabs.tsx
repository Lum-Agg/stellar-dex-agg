'use client';

import type { ReactNode } from 'react';

export type ProfileTab = 'holdings' | 'history' | 'limits' | 'dca';

const TABS: Array<{
  id: ProfileTab;
  label: string;
  enabled: boolean;
  soon?: boolean;
}> = [
  { id: 'holdings', label: 'Holdings', enabled: true },
  { id: 'history', label: 'LumAgg swaps', enabled: true },
  { id: 'limits', label: 'Limit orders', enabled: false, soon: true },
  { id: 'dca', label: 'DCA', enabled: false, soon: true },
];

export function ProfileTabs({
  active,
  onChange,
  trailing,
}: {
  active: ProfileTab;
  onChange: (tab: ProfileTab) => void;
  trailing?: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between border-b border-[var(--border)]">
      <nav
        className="-mb-px flex gap-5 sm:gap-7 overflow-x-auto pb-0 text-[15px] sm:text-[16px] font-medium"
        aria-label="Portfolio sections"
      >
        {TABS.map((tab) => {
          if (!tab.enabled) {
            return (
              <span
                key={tab.id}
                className="flex shrink-0 items-center gap-2 pb-3 text-[var(--text-muted)]/60 cursor-not-allowed whitespace-nowrap"
                title="Coming soon"
              >
                {tab.label}
                {tab.soon && (
                  <span className="rounded-md border border-[var(--border)] px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-[var(--text-muted)]/70">
                    Soon
                  </span>
                )}
              </span>
            );
          }

          const isActive = active === tab.id;
          return (
            <button
              key={tab.id}
              type="button"
              onClick={() => onChange(tab.id)}
              className={`shrink-0 pb-3 border-b-2 transition-colors whitespace-nowrap ${
                isActive
                  ? 'border-[var(--accent)] text-[var(--text-primary)]'
                  : 'border-transparent text-[var(--text-muted)] hover:text-[var(--text-secondary)]'
              }`}
              aria-current={isActive ? 'page' : undefined}
            >
              {tab.label}
            </button>
          );
        })}
      </nav>
      {trailing && (
        <div className="pb-3 text-[12px] text-[var(--text-muted)] shrink-0">{trailing}</div>
      )}
    </div>
  );
}
