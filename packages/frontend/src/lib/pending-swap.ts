import type { SubmitVia } from './submit-preference';

const KEY = 'lumagg.pendingSwap';

export interface PendingSwap {
  hash: string;
  account: string;
  via: SubmitVia;
  network: 'public';
}

export function readPendingSwap(): PendingSwap | null {
  try {
    const value = JSON.parse(sessionStorage.getItem(KEY) ?? 'null');
    if (
      value &&
      typeof value.hash === 'string' &&
      /^[a-f0-9]{64}$/i.test(value.hash) &&
      typeof value.account === 'string' &&
      /^G[A-Z2-7]{55}$/.test(value.account) &&
      (value.via === 'official' || value.via === 'lumagg') &&
      value.network === 'public'
    ) {
      return { hash: value.hash, account: value.account, via: value.via, network: 'public' };
    }
  } catch {
    /* Storage can be unavailable in private browsing. */
  }
  return null;
}

export function savePendingSwap(value: PendingSwap): void {
  try {
    sessionStorage.setItem(KEY, JSON.stringify(value));
  } catch {
    /* Keep tracking in memory. */
  }
}

export function clearPendingSwap(hash: string): void {
  try {
    if (readPendingSwap()?.hash === hash) sessionStorage.removeItem(KEY);
  } catch {
    /* Storage can be unavailable. */
  }
}
