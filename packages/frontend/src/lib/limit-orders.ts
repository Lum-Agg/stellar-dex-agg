/**
 * Testnet-only limit order API client (create / list / cancel).
 * Instant swap continues to use aggregator.ts → NEXT_PUBLIC_API_URL.
 */

import { Networks } from '@creit.tech/stellar-wallets-kit/types';
import { decimalToAtomicUnits } from '@/lib/balance';
import { fetchLatestLedger as fetchLatestLedgerRpc, submitSignedTransaction } from '@/lib/rpc';

/** Minimal token shape shared with TokenSelector (avoid circular imports). */
export interface LimitToken {
  id: string;
  symbol: string;
  name: string;
  decimals: number;
  color: string;
  logo?: string;
}

export const LIMIT_API_URL = process.env.NEXT_PUBLIC_LIMIT_API_URL?.trim() || '';
export const LIMIT_NETWORK_PASSPHRASE = Networks.TESTNET;

/** Well-known testnet SACs for the Limit panel (not mainnet TokenSelector list). */
export const TESTNET_TOKENS: LimitToken[] = [
  {
    id: 'CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV',
    symbol: 'XLM',
    name: 'Stellar Lumens (testnet)',
    decimals: 7,
    color: '#14B8A6',
  },
  {
    id: 'CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA',
    symbol: 'USDC',
    name: 'USD Coin (testnet)',
    decimals: 7,
    color: '#2775CA',
  },
];

export function isLimitApiConfigured(): boolean {
  return LIMIT_API_URL.length > 0;
}

export interface LimitOrder {
  orderId: number;
  owner: string;
  tokenIn: string;
  tokenOut: string;
  amountInInitial?: string;
  amountInRemaining: string;
  limitOutPerInE7: string;
  expiresLedger: number;
  status: string;
  updatedLedger: number;
  updatedAt: number;
}

export interface BuildOrderTxResult {
  unsignedTxXdr: string;
  contract?: string;
}

/** OUT per 1 whole IN → e7 fixed-point string (adjusts for decimal mismatch). */
export function priceHumanToE7(
  priceHuman: string,
  decimalsIn: number,
  decimalsOut: number,
): string {
  const cleaned = priceHuman.trim();
  if (!cleaned || cleaned === '.') throw new Error('Invalid price');
  const n = Number(cleaned);
  if (!Number.isFinite(n) || n <= 0) throw new Error('Price must be positive');
  // price_human = out_units / in_units (whole tokens).
  // e7 stores out_stroops * 1e7 / in_stroops for equal-decimal SACs (both 7).
  // General: out_stroops/in_stroops * 1e7 = price * 10^(decOut-decIn) * 1e7
  const scale = 10 ** (decimalsOut - decimalsIn);
  const e7 = Math.round(n * scale * 1e7);
  if (!Number.isFinite(e7) || e7 <= 0) throw new Error('Price out of range');
  return String(e7);
}

export function e7ToPriceHuman(
  e7: string,
  decimalsIn: number,
  decimalsOut: number,
): string {
  const raw = Number(e7);
  if (!Number.isFinite(raw) || raw <= 0) return '—';
  const scale = 10 ** (decimalsOut - decimalsIn);
  const human = raw / 1e7 / scale;
  return human.toLocaleString(undefined, { maximumFractionDigits: 8 });
}

export function amountToStroops(amount: string, decimals: number): string {
  return decimalToAtomicUnits(amount, decimals);
}

export function formatStroops(stroops: string, decimals: number): string {
  try {
    const n = BigInt(stroops);
    const base = BigInt(10) ** BigInt(decimals);
    const whole = n / base;
    const frac = n % base;
    const fracStr = frac.toString().padStart(decimals, '0').replace(/0+$/, '');
    return fracStr ? `${whole}.${fracStr}` : whole.toString();
  } catch {
    return stroops;
  }
}

export const EXPIRY_PRESETS = [
  { id: '1h', label: '1h', ledgers: 720 },
  { id: '1d', label: '1d', ledgers: 17_280 },
  { id: '7d', label: '7d', ledgers: 120_960 },
] as const;

export type ExpiryPresetId = (typeof EXPIRY_PRESETS)[number]['id'];

export async function fetchLatestLedger(): Promise<number> {
  return fetchLatestLedgerRpc(LIMIT_API_URL);
}

export async function listOpenOrders(user: string): Promise<LimitOrder[]> {
  if (!isLimitApiConfigured()) throw new Error('Limit API not configured');
  const search = new URLSearchParams({ user, status: 'open' });
  const resp = await fetch(`${LIMIT_API_URL}/api/v1/orders?${search}`);
  const json = (await resp.json()) as {
    success?: boolean;
    error?: string;
    data?: { orders?: Record<string, unknown>[] };
  };
  if (!json.success) throw new Error(json.error || 'Failed to list orders');
  return (json.data?.orders || []).map((r) => ({
    orderId: Number(r.order_id ?? 0),
    owner: String(r.owner ?? ''),
    tokenIn: String(r.token_in ?? ''),
    tokenOut: String(r.token_out ?? ''),
    amountInInitial: r.amount_in_initial != null ? String(r.amount_in_initial) : undefined,
    amountInRemaining: String(r.amount_in_remaining ?? '0'),
    limitOutPerInE7: String(r.limit_out_per_in_e7 ?? '0'),
    expiresLedger: Number(r.expires_ledger ?? 0),
    status: String(r.status ?? ''),
    updatedLedger: Number(r.updated_ledger ?? 0),
    updatedAt: Number(r.updated_at ?? 0),
  }));
}

export async function buildCreateOrder(params: {
  user: string;
  tokenIn: string;
  tokenOut: string;
  amountIn: string;
  limitOutPerInE7: string;
  expiresLedger: number;
}): Promise<BuildOrderTxResult> {
  if (!isLimitApiConfigured()) throw new Error('Limit API not configured');
  const resp = await fetch(`${LIMIT_API_URL}/api/v1/orders/build_create`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      user: params.user,
      token_in: params.tokenIn,
      token_out: params.tokenOut,
      amount_in: params.amountIn,
      limit_out_per_in_e7: params.limitOutPerInE7,
      expires_ledger: params.expiresLedger,
    }),
  });
  const json = (await resp.json()) as {
    success?: boolean;
    error?: string;
    data?: Record<string, unknown>;
  };
  if (!json.success || !json.data) throw new Error(json.error || 'build_create failed');
  return {
    unsignedTxXdr: String(json.data.unsigned_tx_xdr ?? ''),
    contract: json.data.contract != null ? String(json.data.contract) : undefined,
  };
}

export async function buildCancelOrder(params: {
  user: string;
  orderId: number;
}): Promise<BuildOrderTxResult> {
  if (!isLimitApiConfigured()) throw new Error('Limit API not configured');
  const resp = await fetch(`${LIMIT_API_URL}/api/v1/orders/build_cancel`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      user: params.user,
      order_id: params.orderId,
    }),
  });
  const json = (await resp.json()) as {
    success?: boolean;
    error?: string;
    data?: Record<string, unknown>;
  };
  if (!json.success || !json.data) throw new Error(json.error || 'build_cancel failed');
  return {
    unsignedTxXdr: String(json.data.unsigned_tx_xdr ?? ''),
    contract: json.data.contract != null ? String(json.data.contract) : undefined,
  };
}

/** Submit signed XDR through limit api-server (or official testnet RPC if Advanced). */
export async function submitLimitTx(signedXdr: string): Promise<{ hash: string }> {
  const result = await submitSignedTransaction(signedXdr, {
    apiUrl: LIMIT_API_URL,
    network: 'testnet',
  });
  if (result.success) {
    return { hash: result.hash };
  }
  throw new Error(result.error || 'Submit failed');
}

export function shortContract(id: string): string {
  if (id.length < 12) return id;
  return `${id.slice(0, 4)}…${id.slice(-4)}`;
}

export function tokenSymbol(id: string, tokens: LimitToken[]): string {
  return tokens.find((t) => t.id === id)?.symbol ?? shortContract(id);
}
