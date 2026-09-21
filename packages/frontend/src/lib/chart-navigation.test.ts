import { describe, expect, it } from 'vitest';
import { getChartNavigationIndex } from './chart-navigation';

describe('getChartNavigationIndex', () => {
  it('moves between adjacent bars with arrow keys', () => {
    expect(getChartNavigationIndex('ArrowLeft', 2, 5)).toBe(1);
    expect(getChartNavigationIndex('ArrowUp', 2, 5)).toBe(1);
    expect(getChartNavigationIndex('ArrowRight', 2, 5)).toBe(3);
    expect(getChartNavigationIndex('ArrowDown', 2, 5)).toBe(3);
  });

  it('does not move beyond the first or last bar', () => {
    expect(getChartNavigationIndex('ArrowLeft', 0, 5)).toBe(0);
    expect(getChartNavigationIndex('ArrowRight', 4, 5)).toBe(4);
  });

  it('supports Home and End', () => {
    expect(getChartNavigationIndex('Home', 2, 5)).toBe(0);
    expect(getChartNavigationIndex('End', 2, 5)).toBe(4);
  });

  it('ignores unrelated keys and invalid ranges', () => {
    expect(getChartNavigationIndex('Enter', 2, 5)).toBeNull();
    expect(getChartNavigationIndex('ArrowRight', 0, 0)).toBeNull();
    expect(getChartNavigationIndex('ArrowRight', -1, 5)).toBeNull();
  });
});
