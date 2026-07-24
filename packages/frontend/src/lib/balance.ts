/** Token balance lookup (stroops) for swap UI and pre-swap checks. */

import { NATIVE_CONTRACT } from '@/lib/tokenDisplay';

const NATIVE_SAC = NATIVE_CONTRACT;
const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';

/** Reserve native XLM for fees when using Max / 100%. */
export const NATIVE_FEE_RESERVE_STROOPS = BigInt(5_000_000); // 0.5 XLM

export type BalanceMap = Record<string, bigint>;
export type TrustlineMap = Record<string, boolean>;

export interface AccountBalancesPayload {
  balances: BalanceMap;
  hasTrustline: TrustlineMap;
  tokensQueried: string[];
  scope: string;
}

export interface TokenBalanceResult {
  balance: bigint;
  hasTrustline: boolean | null;
}

/** Single SAC balance (any token — used for uncommon assets). */
export async function fetchTokenBalance(
  accountId: string,
  tokenContractId: string,
): Promise<TokenBalanceResult | null> {
  const account = accountId.trim();
  const token = tokenContractId.trim();
  if (!account || !token) return null;
  try {
    const params = new URLSearchParams({ account, token });
    const resp = await fetch(`${API_URL}/api/v1/balance?${params}`);
    if (!resp.ok) return null;
    const data = (await resp.json()) as {
      success?: boolean;
      balance?: string;
      has_trustline?: boolean;
    };
    if (data.success && data.balance !== undefined) {
      return {
        balance: BigInt(data.balance),
        hasTrustline: data.has_trustline ?? null,
      };
    }
    return null;
  } catch {
    return null;
  }
}

export async function fetchTokenBalanceStroops(
  accountId: string,
  tokenContractId: string,
): Promise<bigint | null> {
  const result = await fetchTokenBalance(accountId, tokenContractId);
  return result?.balance ?? null;
}

/** Batch SAC balances via Soroban RPC. `common` ≈15 hubs (fast); `catalog` = full set. */
export async function fetchAccountBalances(
  accountId: string,
  scope: 'common' | 'catalog' = 'catalog',
): Promise<AccountBalancesPayload> {
  const params = new URLSearchParams({ account: accountId, scope });
  const resp = await fetch(`${API_URL}/api/v1/balances?${params}`);
  if (!resp.ok) {
    throw new Error('Failed to fetch balances');
  }

  const data = (await resp.json()) as {
    success?: boolean;
    scope?: string;
    tokens_queried?: string[];
    balances?: Record<string, string>;
    has_trustline?: Record<string, boolean>;
  };

  if (!data.success || !data.balances || !data.tokens_queried) {
    throw new Error('Invalid balances response');
  }

  const balances: BalanceMap = {};
  const hasTrustline: TrustlineMap = { ...data.has_trustline };
  // API only includes non-zero amounts; do not synthesize zeros for the whole catalog.
  for (const [tokenId, raw] of Object.entries(data.balances)) {
    balances[tokenId] = BigInt(raw);
  }

  return {
    balances,
    hasTrustline,
    tokensQueried: data.tokens_queried,
    scope: data.scope ?? scope,
  };
}

export async function fetchSpendableBalanceStroops(
  accountId: string,
  tokenContractId: string,
  _decimals: number,
  cached?: BalanceMap | null,
): Promise<bigint | null> {
  if (cached && cached[tokenContractId] !== undefined) {
    return cached[tokenContractId];
  }

  return fetchTokenBalanceStroops(accountId, tokenContractId);
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
  tokenContractId: string,
): bigint {
  let available = balanceStroops;
  if (tokenContractId === NATIVE_SAC && percent >= 100) {
    available =
      balanceStroops > NATIVE_FEE_RESERVE_STROOPS
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
  tokenContractId: string,
): string {
  const stroops = spendableForPercent(balanceStroops, percent, tokenContractId);
  if (stroops === BigInt(0)) return '';
  return stroopsToDecimalString(stroops, decimals);
}
