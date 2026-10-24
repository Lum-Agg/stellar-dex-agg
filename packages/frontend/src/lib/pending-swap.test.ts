import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  clearPendingSwap,
  readPendingSwap,
  savePendingSwap,
  type PendingSwap,
} from './pending-swap';

const pending: PendingSwap = {
  hash: 'ab'.repeat(32),
  account: `G${'A'.repeat(55)}`,
  via: 'lumagg',
  network: 'public',
};
let storage: Map<string, string>;
beforeEach(() => {
  storage = new Map();
  vi.stubGlobal('sessionStorage', {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key),
  });
});
afterEach(() => vi.unstubAllGlobals());

describe('pending swap recovery', () => {
  it('keeps the hash, account and original submission channel', () => {
    savePendingSwap(pending);
    expect(readPendingSwap()).toEqual(pending);
    clearPendingSwap('cd'.repeat(32));
    expect(readPendingSwap()).toEqual(pending);
    clearPendingSwap(pending.hash);
    expect(readPendingSwap()).toBeNull();
  });

  it('rejects malformed storage, unknown networks and unsafe/coerced fields', () => {
    for (const raw of [
      'not json',
      'null',
      '{}',
      JSON.stringify({ ...pending, hash: 'http://example.com' }),
      JSON.stringify({ ...pending, hash: [pending.hash] }),
      JSON.stringify({ ...pending, via: 'other' }),
      JSON.stringify({ ...pending, network: 'testnet' }),
    ]) {
      storage.set('lumagg.pendingSwap', raw);
      expect(readPendingSwap()).toBeNull();
    }
  });

  it('does not crash when storage is unavailable', () => {
    vi.stubGlobal('sessionStorage', {
      getItem: () => {
        throw new Error('unavailable');
      },
      setItem: () => {
        throw new Error('unavailable');
      },
    });
    expect(readPendingSwap()).toBeNull();
    expect(() => savePendingSwap(pending)).not.toThrow();
    expect(() => clearPendingSwap(pending.hash)).not.toThrow();
  });
});
