export function activitySplit(functions: Record<string, number>, total: number) {
  const swaps = functions.swap ?? 0;
  const arbitrage = functions.round_trip_swap ?? 0;
  return {
    swaps,
    arbitrage,
    other: Math.max(0, total - swaps - arbitrage),
    swapShare: total > 0 ? (swaps / total) * 100 : null,
    arbitrageShare: total > 0 ? (arbitrage / total) * 100 : null,
  };
}
