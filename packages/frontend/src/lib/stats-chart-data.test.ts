import { describe, expect, it } from 'vitest';
import { selectChartPeriod, statsChartData, type StatsChartDay } from './stats-chart-data';

const day: StatsChartDay = {
  day: '2026-10-29',
  tx_count: 100,
  unique_users: 3,
  success_count: 100,
  failed_count: 9,
  by_function: { swap: 2, round_trip_swap: 97, unknown: 1 },
};

describe('statsChartData', () => {
  it('uses UTC calendar days, not the last N records, for every chart', () => {
    const days = [
      { day: '2026-09-01' },
      { day: '2026-10-15' },
      { day: '2026-10-16' },
      { day: '2026-10-29' },
    ];
    expect(selectChartPeriod(days, 14)).toEqual(days.slice(2));
    expect(selectChartPeriod(days, 'all')).toEqual(days);
    expect(selectChartPeriod([], 30)).toEqual([]);
  });
  it('keeps user swaps, arbitrage, and other activity separate', () => {
    expect(statsChartData([day])[0]).toMatchObject({
      swaps: 2,
      arbitrage: 97,
      other: 1,
      success: 100,
      failed: 9,
      callers: 3,
    });
  });

  it('does not fill missing prices with zero', () => {
    expect(statsChartData([day])[0]).toMatchObject({ routed: null, notional: null, surplus: null });
  });

  it('leaves absent calendar days as gaps instead of compressing the time axis', () => {
    const points = statsChartData([{ ...day, day: '2026-10-27' }, day]);
    expect(points.map((point) => point.day)).toEqual(['2026-10-27', '2026-10-28', '2026-10-29']);
    expect(points[1]).toMatchObject({ swaps: null, arbitrage: null, callers: null, routed: null });
  });

  it('preserves real zero and negative gross surplus', () => {
    expect(
      statsChartData([
        {
          ...day,
          total_amount_in_usd: 0,
          total_routed_dex_volume_usd: 0,
          round_trip_gross_surplus_usd: -0.1,
        },
      ])[0],
    ).toMatchObject({ routed: 0, notional: 0, surplus: -0.1 });
  });

  it('rejects non-finite chart values', () => {
    expect(
      statsChartData([
        { ...day, total_amount_in_usd: NaN, total_routed_dex_volume_usd: Infinity },
      ])[0],
    ).toMatchObject({ routed: null, notional: null });
    expect(statsChartData([])).toEqual([]);
  });
});
