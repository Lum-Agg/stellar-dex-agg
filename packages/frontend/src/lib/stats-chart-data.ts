import { activitySplit } from './stats-activity';

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
}

function finiteOrNull(value: number | null | undefined) {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
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
  swaps: number | null;
  arbitrage: number | null;
  other: number | null;
  success: number | null;
  failed: number | null;
  callers: number | null;
  routed: number | null;
  notional: number | null;
  surplus: number | null;
};

export function statsChartData(days: StatsChartDay[]): ChartPoint[] {
  const rows: ChartPoint[] = days.map((day) => {
    const split = activitySplit(day.by_function, day.tx_count);
    return {
      day: day.day,
      swaps: split.swaps,
      arbitrage: split.arbitrage,
      other: split.other,
      success: day.success_count,
      failed: day.failed_count,
      callers: day.unique_users,
      routed: finiteOrNull(day.total_routed_dex_volume_usd),
      notional: finiteOrNull(day.total_amount_in_usd),
      surplus: finiteOrNull(day.round_trip_gross_surplus_usd),
    };
  });
  if (rows.length < 2) return rows;
  const byDay = new Map(rows.map((row) => [row.day, row]));
  const dates = [...byDay.keys()].sort();
  const result: ChartPoint[] = [];
  const end = Date.parse(`${dates[dates.length - 1]}T00:00:00Z`);
  // Keep time spacing truthful without inventing zero activity on absent days.
  for (let time = Date.parse(`${dates[0]}T00:00:00Z`); time <= end; time += 86_400_000) {
    const day = new Date(time).toISOString().slice(0, 10);
    result.push(
      byDay.get(day) ?? {
        day,
        swaps: null,
        arbitrage: null,
        other: null,
        success: null,
        failed: null,
        callers: null,
        routed: null,
        notional: null,
        surplus: null,
      },
    );
  }
  return result;
}
