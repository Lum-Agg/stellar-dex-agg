/**
 * Aggregator API client for the frontend.
 */

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';

export interface SubRoute {
  source: string;
  path: string[];
  pool_addresses: string[];
  dex_types: string[];
  in_indices: number[];
  out_indices: number[];
  amount_in: string;
  amount_out: string;
  percentage: number;
}

export interface QuoteData {
  amount_in?: string;
  expected_output: string;
  minimum_output: string;
  price_impact: number;
  is_split: boolean;
  sub_routes: SubRoute[];
  compute_time_ms: number;
}

export interface QuoteResponse {
  success: boolean;
  data?: QuoteData;
  error?: string;
}

/** Raw API row may omit indices or use camelCase; pad to pool hop count. */
function normalizeSubRoute(raw: Record<string, unknown>): SubRoute {
  const pool_addresses =
    (raw.pool_addresses as string[] | undefined) ??
    (raw.poolAddresses as string[] | undefined) ??
    [];
  const n = pool_addresses.length;
  const path = (raw.path as string[] | undefined) ?? [];
  let in_indices =
    (raw.in_indices as number[] | undefined) ?? (raw.inIndices as number[] | undefined) ?? [];
  let out_indices =
    (raw.out_indices as number[] | undefined) ?? (raw.outIndices as number[] | undefined) ?? [];
  let dex_types =
    (raw.dex_types as string[] | undefined) ?? (raw.dexTypes as string[] | undefined) ?? [];
  in_indices = [...in_indices];
  out_indices = [...out_indices];
  dex_types = [...dex_types];
  while (in_indices.length < n) in_indices.push(0);
  while (out_indices.length < n) out_indices.push(1);
  while (dex_types.length < n) dex_types.push('aquarius');

  return {
    source: String(raw.source ?? ''),
    path,
    pool_addresses,
    dex_types,
    in_indices,
    out_indices,
    amount_in: String(raw.amount_in ?? raw.amountIn ?? '0'),
    amount_out: String(raw.amount_out ?? raw.amountOut ?? '0'),
    percentage: Number(raw.percentage ?? 0),
  };
}

function normalizeQuoteData(data: QuoteData): QuoteData {
  const sub_routes = (data.sub_routes ?? []).map((r) =>
    normalizeSubRoute(r as unknown as Record<string, unknown>)
  );
  const subSum = sub_routes.reduce((s, r) => s + BigInt(r.amount_in || '0'), 0n);
  const amount_in =
    data.amount_in ??
    (subSum > 0n ? subSum.toString() : undefined);
  return { ...data, sub_routes, amount_in };
}

export async function getQuote(
  tokenIn: string,
  tokenOut: string,
  amountIn: string,
  slippage?: number,
  signal?: AbortSignal
): Promise<QuoteResponse> {
  const params = new URLSearchParams({
    token_in: tokenIn,
    token_out: tokenOut,
    amount_in: amountIn,
  });
  if (slippage !== undefined) {
    params.set('slippage', slippage.toString());
  }

  const resp = await fetch(`${API_URL}/api/v1/quote?${params}`, { signal });
  const json = (await resp.json()) as QuoteResponse;
  if (json.success && json.data) {
    json.data = normalizeQuoteData(json.data);
  }
  return json;
}

export async function buildSwap(
  tokenIn: string,
  tokenOut: string,
  amountIn: string,
  slippage: number,
  userPublicKey: string
): Promise<{ success: boolean; data?: { unsigned_tx_xdr: string }; error?: string }> {
  const resp = await fetch(`${API_URL}/api/v1/swap`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      token_in: tokenIn,
      token_out: tokenOut,
      amount_in: amountIn,
      slippage,
      user_public_key: userPublicKey,
    }),
  });
  return resp.json();
}
