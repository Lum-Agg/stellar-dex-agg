/**
 * Aggregator API client for the frontend.
 */

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';

export interface SubRoute {
  source: string;
  path: string[];
  pool_addresses: string[];
  dex_types: string[];
  amount_in: string;
  amount_out: string;
  percentage: number;
}

export interface QuoteData {
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

export async function getQuote(
  tokenIn: string,
  tokenOut: string,
  amountIn: string,
  slippage?: number
): Promise<QuoteResponse> {
  const params = new URLSearchParams({
    token_in: tokenIn,
    token_out: tokenOut,
    amount_in: amountIn,
  });
  if (slippage !== undefined) {
    params.set('slippage', slippage.toString());
  }

  const resp = await fetch(`${API_URL}/api/v1/quote?${params}`);
  return resp.json();
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
