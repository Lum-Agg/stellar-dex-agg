import { fetchJson } from '@/lib/fetch-json';

export const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';

export interface Price {
  price_usdc: number;
  ts: number;
  via: string;
}

export interface PriceHistoryPoint {
  ts: number;
  price_usdc: number;
}

export async function fetchPrices(ids: string[]): Promise<Map<string, Price>> {
  if (ids.length === 0) return new Map();

  const params = new URLSearchParams({ ids: ids.join(',') });
  const json = await fetchJson<{
    success?: boolean;
    error?: string;
    data?: { prices?: Array<{ id?: string; price_usdc?: number; ts?: number; via?: string }> };
  }>(`${API_URL}/api/v1/prices?${params}`);

  if (!json.success) {
    throw new Error(json.error || 'Failed to fetch prices');
  }

  return new Map(
    (json.data?.prices ?? [])
      .filter(
        (price): price is Required<typeof price> =>
          typeof price.id === 'string' &&
          typeof price.price_usdc === 'number' &&
          typeof price.ts === 'number' &&
          typeof price.via === 'string',
      )
      .map((price) => [
        price.id,
        {
          price_usdc: price.price_usdc,
          ts: price.ts,
          via: price.via,
        },
      ]),
  );
}

export async function fetchPriceHistory(
  id: string,
  range: '24h' | '7d' = '24h',
): Promise<PriceHistoryPoint[]> {
  const params = new URLSearchParams({ id, range });
  const json = await fetchJson<{
    success?: boolean;
    error?: string;
    data?: { points?: Array<{ ts?: number; price_usdc?: number }> };
  }>(`${API_URL}/api/v1/prices/history?${params}`);

  if (!json.success) {
    throw new Error(json.error || 'Failed to fetch price history');
  }

  return (json.data?.points ?? [])
    .filter(
      (point): point is Required<typeof point> =>
        typeof point.ts === 'number' && typeof point.price_usdc === 'number',
    )
    .map(({ ts, price_usdc }) => ({ ts, price_usdc }));
}
