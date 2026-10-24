import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  clearPendingSwap,
  readPendingSwap,
  savePendingSwap,
  signedSwapHash,
  type PendingSwap,
} from './pending-swap';
import {
  Account,
  Asset,
  Networks,
  Operation,
  StrKey,
  TransactionBuilder,
} from '@stellar/stellar-sdk/minimal';

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
  it('derives the mainnet envelope hash locally, including fee-bump envelopes', async () => {
    const address = StrKey.encodeEd25519PublicKey(Buffer.alloc(32, 7));
    const tx = new TransactionBuilder(new Account(address, '1'), {
      fee: '100',
      networkPassphrase: Networks.PUBLIC,
    })
      .addOperation(Operation.payment({ destination: address, asset: Asset.native(), amount: '1' }))
      .setTimeout(60)
      .build();
    expect(await signedSwapHash(tx.toXDR())).toBe(tx.hash().toString('hex'));
    const feeBump = TransactionBuilder.buildFeeBumpTransaction(address, '200', tx, Networks.PUBLIC);
    expect(await signedSwapHash(feeBump.toXDR())).toBe(feeBump.hash().toString('hex'));
    expect(await signedSwapHash(feeBump.toXDR())).not.toBe(tx.hash().toString('hex'));
  });

  it('rejects invalid envelopes before broadcasting', async () => {
    await expect(signedSwapHash('invalid')).rejects.toThrow();
  });

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
