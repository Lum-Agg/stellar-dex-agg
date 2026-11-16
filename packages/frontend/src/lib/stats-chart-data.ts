import { activitySplit } from './stats-activity';
import { NATIVE_CONTRACT, USDC_CONTRACT } from './tokenDisplay';

export interface StatsChartDay {
  day: string;
  tx_count: number;
  unique_users: number;
  success_count: number;
  failed_count: number;
  by_function: Record<string, number>;
  total_amount_in_usd?: number | null;
  total_routed_dex_volume_usd?: number | null;
  round_trip_gross_surplus_usd?: number | null;
  round_trip_by_token?: Array<{
    base_token: string;
    gross_surplus: string | number;
  }>;
}

function finiteOrZero(value: number | null | undefined) {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0;
}

export function selectChartPeriod<T extends { day: string }>(
  days: T[],
  range: number | 'all',
): T[] {
  if (range === 'all' || days.length === 0) return days;
  const latest = Math.max(...days.map((day) => Date.parse(`${day.day}T00:00:00Z`)));
  const cutoff = latest - (range - 1) * 86_400_000;
  return days.filter((day) => Date.parse(`${day.day}T00:00:00Z`) >= cutoff);
}

type ChartPoint = {
  day: string;
  swaps: number;
  arbitrage: number;
  success: number;
  failed: number;
  callers: number;
  routed: number;
  notional: number;
  surplus: number;
  surplusXlm: number;
  surplusUsdc: number;
};

function tokenSurplus(day: StatsChartDay, token: string): number {
  const raw = day.round_trip_by_token?.find((row) => row.base_token === token)?.gross_surplus;
  const units = typeof raw === 'string' && /^-?\d+$/.test(raw) ? Number(raw) : raw;
  return typeof units === 'number' && Number.isInteger(units) ? units / 10_000_000 : 0;
}

export function statsChartData(days: StatsChartDay[]): ChartPoint[] {
  const rows: ChartPoint[] = days.map((day) => {
    const split = activitySplit(day.by_function, day.tx_count);
    return {
      day: day.day,
      swaps: split.swaps,
      arbitrage: split.arbitrage,
      success: finiteOrZero(day.success_count),
      failed: finiteOrZero(day.failed_count),
      callers: finiteOrZero(day.unique_users),
      routed: finiteOrZero(day.total_routed_dex_volume_usd),
      notional: finiteOrZero(day.total_amount_in_usd),
      surplus: finiteOrZero(day.round_trip_gross_surplus_usd),
      surplusXlm: tokenSurplus(day, NATIVE_CONTRACT),
      surplusUsdc: tokenSurplus(day, USDC_CONTRACT),
    };
  });
  if (rows.length < 2) return rows;
  const byDay = new Map(rows.map((row) => [row.day, row]));
  const dates = [...byDay.keys()].sort();
  const result: ChartPoint[] = [];
  const end = Date.parse(`${dates[dates.length - 1]}T00:00:00Z`);
  // Zero-fill the chart timeline without changing the source daily records.
  for (let time = Date.parse(`${dates[0]}T00:00:00Z`); time <= end; time += 86_400_000) {
    const day = new Date(time).toISOString().slice(0, 10);
    result.push(
      byDay.get(day) ?? {
        day,
        swaps: 0,
        arbitrage: 0,
        success: 0,
        failed: 0,
        callers: 0,
        routed: 0,
        notional: 0,
        surplus: 0,
        surplusXlm: 0,
        surplusUsdc: 0,
      },
    );
  }
  return result;
}
