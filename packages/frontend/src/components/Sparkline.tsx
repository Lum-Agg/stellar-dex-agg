'use client';

import type { PriceHistoryPoint } from '@/lib/prices';

export function Sparkline({ points }: { points: PriceHistoryPoint[] }) {
  if (points.length < 3) {
    return <span className="text-[12px] text-zinc-600">—</span>;
  }

  const values = points.map((point) => point.price_usdc);
  const low = Math.min(...values);
  const high = Math.max(...values);
  const range = high - low || 1;
  const coordinates = values
    .map((value, index) => {
      const x = 2 + (index / (values.length - 1)) * 76;
      const y = 26 - ((value - low) / range) * 24;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(' ');
  const stroke = values.at(-1)! >= values[0] ? '#34d399' : '#a1a1aa';

  return (
    <svg
      width="80"
      height="28"
      viewBox="0 0 80 28"
      role="img"
      aria-label="24 hour price history"
      className="shrink-0"
    >
      <polyline
        points={coordinates}
        fill="none"
        stroke={stroke}
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
