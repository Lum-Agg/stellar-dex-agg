import { describe, expect, it } from 'vitest';
import { activitySplit } from './stats-activity';

describe('activitySplit', () => {
  it('reports the user / arbitrage split against all successful invocations', () => {
    expect(activitySplit({ swap: 2, round_trip_swap: 98 }, 100)).toEqual({
      swaps: 2,
      arbitrage: 98,
      other: 0,
      swapShare: 2,
      arbitrageShare: 98,
    });
  });

  it('does not label unknown functions as user swaps', () => {
    expect(activitySplit({ swap: 1, round_trip_swap: 2, future: 1 }, 4)).toEqual({
      swaps: 1,
      arbitrage: 2,
      other: 1,
      swapShare: 25,
      arbitrageShare: 50,
    });
  });

  it('does not infer arbitrage counts from complete-return surplus records', () => {
    expect(activitySplit({ round_trip_swap: 3 }, 3).arbitrage).toBe(3);
  });

  it('leaves percentages unavailable when no successful invocations exist', () => {
    expect(activitySplit({}, 0).swapShare).toBeNull();
    expect(activitySplit({}, 0).arbitrageShare).toBeNull();
  });
});
