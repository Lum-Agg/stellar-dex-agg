/** Token balance lookup (stroops) for swap UI and pre-swap checks. */

import { NATIVE_CONTRACT } from '@/lib/tokenDisplay';

const HORIZON = 'https://horizon.stellar.org';
const NATIVE_SAC = NATIVE_CONTRACT;
const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';

/** Reserve native XLM for fees when using Max / 100%. */
export const NATIVE_FEE_RESERVE_STROOPS = BigInt(5_000_000); // 0.5 XLM

export type BalanceMap = Record<string, bigint>;

export interface AccountBalancesPayload {
  balances: BalanceMap;
  tokensQueried: string[];
  scope: string;
}

function horizonBalanceToStroops(balance: string, decimals: number): bigint {
  const parts = balance.split('.');
  const whole = parts[0] || '0';
  const frac = (parts[1] ?? '').padEnd(decimals, '0').slice(0, decimals);
  return BigInt(whole) * BigInt(10 ** decimals) + BigInt(frac || '0');
}

/** Single SAC balance (any token — used for uncommon assets). */
export async function fetchTokenBalanceStroops(
  accountId: string,
  tokenContractId: string
): Promise<bigint | null> {
  try {
    const params = new URLSearchParams({ account: accountId, token: tokenContractId });
    const resp = await fetch(`${API_URL}/api/v1/balance?${params}`);
    if (!resp.ok) return null;
    const data = (await resp.json()) as { success?: boolean; balance?: string };
    if (data.success && data.balance !== undefined) return BigInt(data.balance);
    return null;
  } catch {
    return null;
  }
}

async function fetchNativeHorizonBalance(
  accountId: string,
  decimals: number
): Promise<bigint | null> {
  try {
    const resp = await fetch(`${HORIZON}/accounts/${accountId}`);
    if (!resp.ok) return null;
    const data = (await resp.json()) as {
      balances?: Array<{ balance?: string; asset_type?: string }>;
    };

    for (const b of data.balances ?? []) {
      if (b.asset_type === 'native' && b.balance) {
        return horizonBalanceToStroops(b.balance, decimals);
      }
    }
    return null;
  } catch {
    return null;
  }
}

/** Batch fetch curated common tokens (`scope=common` on api-server). */
export async function fetchAccountBalances(accountId: string): Promise<AccountBalancesPayload> {
  const params = new URLSearchParams({ account: accountId });
  const resp = await fetch(`${API_URL}/api/v1/balances?${params}`);
  if (!resp.ok) {
    throw new Error('Failed to fetch balances');
  }

  const data = (await resp.json()) as {
    success?: boolean;
    scope?: string;
    tokens_queried?: string[];
    balances?: Record<string, string>;
  };

  if (!data.success || !data.balances || !data.tokens_queried) {
    throw new Error('Invalid balances response');
  }

  const balances: BalanceMap = {};
  for (const tokenId of data.tokens_queried) {
    const raw = data.balances[tokenId];
    balances[tokenId] = raw !== undefined ? BigInt(raw) : BigInt(0);
  }

  return {
    balances,
    tokensQueried: data.tokens_queried,
    scope: data.scope ?? 'common',
  };
}

export async function fetchSpendableBalanceStroops(
  accountId: string,
  tokenContractId: string,
  decimals: number,
  cached?: BalanceMap | null
): Promise<bigint | null> {
  if (cached && cached[tokenContractId] !== undefined) {
    return cached[tokenContractId];
  }

  const apiBalance = await fetchTokenBalanceStroops(accountId, tokenContractId);
  if (apiBalance !== null) return apiBalance;

  if (tokenContractId === NATIVE_SAC) {
    return fetchNativeHorizonBalance(accountId, decimals);
  }
  return null;
}

export function stroopsToDecimalString(stroops: bigint, decimals: number): string {
  const base = BigInt(10 ** decimals);
  const whole = stroops / base;
  const frac = stroops % base;
  if (frac === BigInt(0)) return whole.toString();
  const fracStr = frac.toString().padStart(decimals, '0').replace(/0+$/, '');
  return `${whole}.${fracStr}`;
}

export function formatBalanceDisplay(stroops: bigint, decimals: number): string {
  const val = Number(stroops) / 10 ** decimals;
  if (val >= 1_000_000) return val.toLocaleString(undefined, { maximumFractionDigits: 2 });
  if (val >= 1) return val.toLocaleString(undefined, { maximumFractionDigits: 4 });
  if (val >= 0.0001) return val.toLocaleString(undefined, { maximumFractionDigits: 6 });
  if (val === 0) return '0';
  return val.toExponential(2);
}

export function spendableForPercent(
  balanceStroops: bigint,
  percent: number,
  tokenContractId: string
): bigint {
  let available = balanceStroops;
  if (tokenContractId === NATIVE_SAC && percent >= 100) {
    available = balanceStroops > NATIVE_FEE_RESERVE_STROOPS
      ? balanceStroops - NATIVE_FEE_RESERVE_STROOPS
      : BigInt(0);
  } else if (percent >= 100) {
    available = balanceStroops;
  } else {
    available = (balanceStroops * BigInt(percent)) / BigInt(100);
  }
  return available > BigInt(0) ? available : BigInt(0);
}

export function percentToAmountInput(
  balanceStroops: bigint,
  percent: number,
  decimals: number,
  tokenContractId: string
): string {
  const stroops = spendableForPercent(balanceStroops, percent, tokenContractId);
  if (stroops === BigInt(0)) return '';
  return stroopsToDecimalString(stroops, decimals);
}
