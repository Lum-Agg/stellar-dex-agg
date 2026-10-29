'use client';

import { useId, type ReactNode } from 'react';
import { ResponsiveContainer } from 'recharts';

export type ChartConfig = Record<string, { label: string; color: string }>;

// Recharts composition, with shared container/tooltip/legend styling rather
// than a second chart engine. Colors inherit the existing LumAgg theme.
export function ChartContainer({ label, children }: { label: string; children: ReactNode }) {
  const id = useId();
  return (
    <div
      data-chart={id}
      aria-label={label}
      role="region"
      className="h-[260px] min-w-0 w-full text-[11px] sm:h-[300px] [&_.recharts-cartesian-axis-tick_text]:fill-[var(--text-muted)] [&_.recharts-cartesian-grid_line]:stroke-[var(--border)] [&_.recharts-surface:focus-visible]:outline-[var(--accent)]"
    >
      <ResponsiveContainer
        width="100%"
        height="100%"
        minWidth={0}
        initialDimension={{ width: 600, height: 300 }}
      >
        {children}
      </ResponsiveContainer>
    </div>
  );
}

export function ChartLegend({ config }: { config: ChartConfig }) {
  return (
    <ul
      aria-label="Chart legend"
      className="flex flex-wrap gap-x-5 gap-y-2 text-[11px] text-[var(--text-secondary)]"
    >
      {Object.entries(config).map(([key, item]) => (
        <li key={key} className="inline-flex items-center gap-2">
          <span className="h-2.5 w-2.5 rounded-[3px]" style={{ background: item.color }} />
          {item.label}
        </li>
      ))}
    </ul>
  );
}

type TooltipEntry = {
  dataKey?: unknown;
  value?: unknown;
};

export function ChartTooltipContent({
  active,
  label,
  payload,
  config,
  formatValue = (value) => value.toLocaleString(),
}: {
  active?: boolean;
  label?: ReactNode;
  payload?: readonly TooltipEntry[];
  config: ChartConfig;
  formatValue?: (value: number) => string;
}) {
  if (!active || !payload?.length) return null;
  return (
    <div
      className="min-w-[190px] rounded-xl border border-[var(--border-strong)] bg-[var(--surface)] px-3.5 py-3 text-xs shadow-xl"
      data-chart-tooltip
    >
      <p className="mb-2.5 border-b border-[var(--border)] pb-2 font-medium text-[var(--text-primary)]">
        {label}
        <span className="ml-2 text-[10px] font-normal text-[var(--text-muted)]">UTC</span>
      </p>
      <dl className="space-y-2">
        {payload.map((item) => {
          const key = String(item.dataKey);
          const entry = config[key];
          if (!entry) return null;
          const value =
            typeof item.value === 'number' && Number.isFinite(item.value) ? item.value : null;
          return (
            <div key={key} className="flex items-center justify-between gap-5">
              <dt className="flex items-center gap-2 text-[var(--text-secondary)]">
                <span className="h-2 w-2 rounded-sm" style={{ background: entry.color }} />
                {entry.label}
              </dt>
              <dd className="font-mono font-medium tabular-nums text-[var(--text-primary)]">
                {value === null ? 'Unavailable' : formatValue(value)}
              </dd>
            </div>
          );
        })}
      </dl>
    </div>
  );
}
