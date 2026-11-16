import { describe, expect, it } from 'vitest';
import { selectChartPeriod, statsChartData, type StatsChartDay } from './stats-chart-data';
import { NATIVE_CONTRACT, USDC_CONTRACT } from './tokenDisplay';

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
  it('charts only user swaps and arbitrage without changing the successful total', () => {
    const point = statsChartData([day])[0];
    expect(point).toMatchObject({
      swaps: 2,
      arbitrage: 97,
      success: 100,
      failed: 9,
      callers: 3,
    });
    expect(point).not.toHaveProperty('other');
  });

  it('plots missing prices and surplus as zero even on days with activity', () => {
    expect(statsChartData([day])[0]).toMatchObject({
      routed: 0, notional: 0, surplus: 0, surplusXlm: 0, surplusUsdc: 0, arbitrage: 97,
    });
  });

  it('plots zero USD on indexed days with only failed transactions', () => {
    const points = statsChartData([
      { ...day, day: '2026-10-27', total_amount_in_usd: 5 },
      {
        ...day,
        day: '2026-10-28',
        tx_count: 0,
        success_count: 0,
        failed_count: 12,
        unique_users: 0,
        by_function: {},
        total_amount_in_usd: null,
        total_routed_dex_volume_usd: null,
        round_trip_gross_surplus_usd: null,
      },
      { ...day, total_amount_in_usd: 10 },
    ]);
    expect(points[1]).toMatchObject({ routed: 0, notional: 0, surplus: 0, failed: 12 });
  });

  it('fills absent calendar days with zero without changing the timeline', () => {
    const points = statsChartData([{ ...day, day: '2026-10-27' }, day]);
    expect(points.map((point) => point.day)).toEqual(['2026-10-27', '2026-10-28', '2026-10-29']);
    expect(points[1]).toMatchObject({
      swaps: 0, arbitrage: 0, callers: 0, routed: 0, notional: 0, surplus: 0,
    });
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

  it('converts raw XLM and USDC surplus independently without USD prices', () => {
    const rows = [
      { base_token: NATIVE_CONTRACT, gross_surplus: '12345678' },
      { base_token: USDC_CONTRACT, gross_surplus: -250000 },
    ];
    const [both, partial] = statsChartData([
      { ...day, day: '2026-10-28', round_trip_by_token: rows },
      { ...day, round_trip_by_token: [rows[0]] },
    ]);
    expect(both).toMatchObject({ surplus: 0, surplusXlm: 1.2345678, surplusUsdc: -0.025 });
    expect(partial).toMatchObject({ surplusXlm: 1.2345678, surplusUsdc: 0 });
  });

  it('rejects non-finite chart values', () => {
    expect(
      statsChartData([
        { ...day, total_amount_in_usd: NaN, total_routed_dex_volume_usd: Infinity },
      ])[0],
    ).toMatchObject({ routed: 0, notional: 0 });
    expect(statsChartData([])).toEqual([]);
  });
});
