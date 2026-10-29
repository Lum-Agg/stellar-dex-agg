'use client';

import { useId, type ReactNode } from 'react';
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Line,
  LineChart,
  ReferenceLine,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import {
  ChartContainer,
  ChartLegend,
  ChartTooltipContent,
  type ChartConfig,
} from '@/components/ui/chart';
import { statsChartData, type StatsChartDay } from '@/lib/stats-chart-data';

const activityConfig = {
  swaps: { label: 'User swaps', color: 'var(--accent)' },
  arbitrage: { label: 'Arbitrage', color: 'var(--chart-blue)' },
  other: { label: 'Other', color: 'var(--chart-neutral)' },
};
const volumeConfig = {
  routed: { label: 'Routed USD', color: 'var(--accent)' },
  notional: { label: 'Entry notional USD', color: 'var(--chart-blue)' },
};
const outcomeConfig = {
  success: { label: 'Successful', color: 'var(--accent)' },
  failed: { label: 'Failed', color: 'var(--chart-rose)' },
};
const callersConfig = { callers: { label: 'Caller addresses', color: 'var(--chart-blue)' } };
const surplusConfig = { surplus: { label: 'Gross surplus USD', color: 'var(--accent)' } };

const axis = { tickLine: false, axisLine: false, tickMargin: 10, fontSize: 11 };
const margin = { top: 18, right: 12, bottom: 4, left: 0 };

function shortDay(day: string) {
  return new Date(`${day}T00:00:00Z`).toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    timeZone: 'UTC',
  });
}

function usd(value: number) {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    maximumFractionDigits: Math.abs(value) < 1 ? 4 : 2,
  }).format(value);
}

function usdTick(value: number) {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    notation: 'compact',
    maximumFractionDigits: 2,
  }).format(value);
}

function ChartCard({
  title,
  eyebrow,
  description,
  config,
  empty,
  emptyMessage = 'No indexed data for this period',
  children,
  footer,
}: {
  title: string;
  eyebrow: string;
  description: string;
  config: ChartConfig;
  empty: boolean;
  emptyMessage?: string;
  children: ReactNode;
  footer?: string;
}) {
  return (
    <section
      className="min-w-0 overflow-hidden rounded-2xl border border-[var(--border)] bg-[var(--surface)] shadow-[0_2px_12px_rgba(0,0,0,0.025)]"
      aria-label={title}
    >
      <div className="space-y-3 px-5 pt-5 sm:px-6 sm:pt-6">
        <div className="text-[10px] font-medium uppercase tracking-[0.18em] text-[var(--text-muted)]">
          {eyebrow}
        </div>
        <div>
          <h2 className="text-[17px] font-medium tracking-tight text-[var(--text-primary)]">
            {title}
          </h2>
          <p className="mt-1.5 max-w-2xl text-xs leading-relaxed text-[var(--text-secondary)]">
            {description}
          </p>
        </div>
        <ChartLegend config={config} />
      </div>
      <div className="px-2 pb-2 pt-4 sm:px-4">
        {empty ? (
          <div className="flex h-[260px] items-center justify-center text-sm text-[var(--text-muted)]">
            {emptyMessage}
          </div>
        ) : (
          <ChartContainer label={`${title} chart`}>{children}</ChartContainer>
        )}
      </div>
      {footer && (
        <p className="border-t border-[var(--border)] bg-[var(--surface-raised)]/30 px-5 py-3 text-[11px] leading-relaxed text-[var(--text-muted)] sm:px-6">
          {footer}
        </p>
      )}
    </section>
  );
}

export function StatsCharts({ days }: { days: StatsChartDay[] }) {
  const data = statsChartData(days);
  const id = useId().replace(/:/g, '');
  const tooltip = (config: ChartConfig, money = false) => (
    <Tooltip
      filterNull={false}
      cursor={{ fill: 'var(--chart-hover)', stroke: 'var(--border-strong)' }}
      content={(props) => (
        <ChartTooltipContent {...props} config={config} formatValue={money ? usd : undefined} />
      )}
    />
  );
  const dates = (
    <XAxis
      {...axis}
      dataKey="day"
      tickFormatter={shortDay}
      minTickGap={36}
      padding={{ left: 12, right: 12 }}
    />
  );
  const grid = <CartesianGrid vertical={false} strokeDasharray="3 5" />;

  return (
    <div className="space-y-5">
      <ChartCard
        title="User swaps vs arbitrage over time"
        eyebrow="01 / Transaction mix"
        description="Successful indexed transactions, separated by contract function. Counts are not a measure of independent users."
        config={activityConfig}
        empty={!data.length}
      >
        <BarChart data={data} margin={margin} accessibilityLayer barCategoryGap="28%">
          {grid}
          {dates}
          <YAxis {...axis} allowDecimals={false} width={44} />
          {tooltip(activityConfig)}
          <Bar
            dataKey="swaps"
            stackId="activity"
            fill={activityConfig.swaps.color}
            maxBarSize={32}
          />
          <Bar
            dataKey="arbitrage"
            stackId="activity"
            fill={activityConfig.arbitrage.color}
            maxBarSize={32}
          />
          <Bar
            dataKey="other"
            stackId="activity"
            fill={activityConfig.other.color}
            maxBarSize={32}
          />
        </BarChart>
      </ChartCard>

      <ChartCard
        title="Daily combined volume (includes arbitrage)"
        eyebrow="02 / Capital routed"
        description="Entry notional and actual DEX-leg input volume, shown on the same USD scale. No transaction-count axis."
        config={volumeConfig}
        empty={!data.length || !data.some((day) => day.routed !== null || day.notional !== null)}
        emptyMessage={data.length ? 'No priced volume available for this period' : undefined}
        footer="Priced subtotals only. Routed volume counts each executed leg, including arbitrage; it is not user-only volume. Gaps mean unavailable data, not zero."
      >
        <AreaChart data={data} margin={margin} accessibilityLayer>
          <defs>
            <linearGradient id={`${id}-volume`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--accent)" stopOpacity={0.22} />
              <stop offset="100%" stopColor="var(--accent)" stopOpacity={0.015} />
            </linearGradient>
          </defs>
          {grid}
          {dates}
          <YAxis {...axis} width={62} tickFormatter={usdTick} />
          {tooltip(volumeConfig, true)}
          <Area
            dataKey="routed"
            type="linear"
            connectNulls={false}
            stroke="var(--accent)"
            strokeWidth={2}
            fill={`url(#${id}-volume)`}
            dot={{ r: data.length <= 2 ? 3 : 0 }}
            activeDot={{ r: 4, strokeWidth: 2, stroke: 'var(--surface)' }}
          />
          <Area
            dataKey="notional"
            type="linear"
            connectNulls={false}
            stroke="var(--chart-blue)"
            strokeWidth={2}
            fill="transparent"
            dot={{ r: data.length <= 2 ? 3 : 0 }}
            activeDot={{ r: 4 }}
          />
        </AreaChart>
      </ChartCard>

      <div className="grid gap-5">
        <ChartCard
          title="Execution outcome"
          eyebrow="03 / Settlement"
          description="Confirmed successful and failed on-chain transactions. Rejected simulations are not included."
          config={outcomeConfig}
          empty={!data.length}
        >
          <BarChart data={data} margin={margin} accessibilityLayer barCategoryGap="30%">
            {grid}
            {dates}
            <YAxis {...axis} width={40} allowDecimals={false} />
            {tooltip(outcomeConfig)}
            <Bar dataKey="success" stackId="outcome" fill="var(--accent)" maxBarSize={24} />
            <Bar dataKey="failed" stackId="outcome" fill="var(--chart-rose)" maxBarSize={24} />
          </BarChart>
        </ChartCard>
        <ChartCard
          title="Caller activity"
          eyebrow="04 / Addresses"
          description="Daily distinct successful caller addresses, including arbitrage operators. Not verified independent users."
          config={callersConfig}
          empty={!data.length}
        >
          <LineChart data={data} margin={margin} accessibilityLayer>
            {grid}
            {dates}
            <YAxis {...axis} width={40} allowDecimals={false} />
            {tooltip(callersConfig)}
            <Line
              dataKey="callers"
              type="linear"
              connectNulls={false}
              stroke="var(--chart-blue)"
              strokeWidth={2}
              dot={{ r: data.length <= 2 ? 3 : 0 }}
              activeDot={{ r: 4 }}
            />
          </LineChart>
        </ChartCard>
      </div>

      <ChartCard
        title="Arbitrage gross surplus"
        eyebrow="05 / Execution surplus"
        description="Daily priced surplus from successful round trips with known return amounts."
        config={surplusConfig}
        empty={!data.length || !data.some((day) => day.surplus !== null)}
        emptyMessage={data.length ? 'No priced surplus available for this period' : undefined}
        footer="Gross surplus excludes transaction fees and is not net P&L. Missing prices or return amounts remain unavailable."
      >
        <AreaChart data={data} margin={margin} accessibilityLayer>
          <defs>
            <linearGradient id={`${id}-surplus`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--accent)" stopOpacity={0.2} />
              <stop offset="100%" stopColor="var(--accent)" stopOpacity={0.015} />
            </linearGradient>
          </defs>
          {grid}
          {dates}
          <YAxis {...axis} width={62} tickFormatter={usdTick} />
          <ReferenceLine y={0} stroke="var(--border-strong)" />
          {tooltip(surplusConfig, true)}
          <Area
            dataKey="surplus"
            type="linear"
            connectNulls={false}
            stroke="var(--accent)"
            strokeWidth={2}
            fill={`url(#${id}-surplus)`}
            dot={{ r: data.length <= 2 ? 3 : 0 }}
            activeDot={{ r: 4 }}
          />
        </AreaChart>
      </ChartCard>
    </div>
  );
}
