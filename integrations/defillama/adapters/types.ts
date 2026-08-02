/**
 * Local IDE stubs for DefiLlama dimension-adapters types.
 * Not shipped in the upstream PR — only mirrors enough for typecheck here.
 */

export interface Balances {
  add(token: string, amount: number | string): void;
  addUSDValue(usd: number, label?: string): void;
}

export interface FetchOptions {
  dateString: string;
  startTimestamp: number;
  endTimestamp: number;
  toTimestamp: number;
  createBalances(): Balances;
}

export interface SimpleAdapter {
  version: number;
  fetch: (options: FetchOptions) => Promise<{ dailyVolume: Balances | number }>;
  chains: string[];
  start: string;
  methodology?: Record<string, string>;
  breakdownMethodology?: Record<string, Record<string, string>>;
  dependencies?: string[];
}
