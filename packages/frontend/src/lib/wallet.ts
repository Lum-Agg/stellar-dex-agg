/**
 * Wallet utilities for signing and submitting Stellar transactions.
 */

import { submitSignedTransaction, type SubmitTxOptions } from '@/lib/rpc';

const NETWORK = 'PUBLIC';

/**
 * Sign a transaction XDR using Freighter.
 */
export async function signTransaction(xdr: string): Promise<string> {
  const freighterApi = await import('@stellar/freighter-api');
  const result = await freighterApi.signTransaction(xdr, { network: NETWORK });

  if (typeof result === 'string') {
    return result;
  }
  if (result && typeof result === 'object' && 'signedTxXdr' in result) {
    return (result as { signedTxXdr: string }).signedTxXdr;
  }
  throw new Error('Failed to sign transaction');
}

/**
 * Submit a signed transaction through api-server.
 */
export async function submitTransaction(
  signedXdr: string,
  opts?: SubmitTxOptions,
): Promise<{
  hash: string;
  success: boolean;
  error?: string;
}> {
  return submitSignedTransaction(signedXdr, opts);
}
