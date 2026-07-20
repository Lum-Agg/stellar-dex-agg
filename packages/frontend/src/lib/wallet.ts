/**
 * Wallet utilities for signing and submitting Stellar transactions.
 */

const NETWORK = 'PUBLIC';
const HORIZON_URL = 'https://horizon.stellar.org';

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
    return (result as any).signedTxXdr;
  }
  throw new Error('Failed to sign transaction');
}

/**
 * Submit a signed transaction to the Stellar network.
 */
export async function submitTransaction(signedXdr: string): Promise<{
  hash: string;
  success: boolean;
  error?: string;
}> {
  const resp = await fetch(`${HORIZON_URL}/transactions`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: `tx=${encodeURIComponent(signedXdr)}`,
  });

  const data = await resp.json();

  if (resp.ok) {
    return { hash: data.hash, success: true };
  } else {
    const error =
      data.extras?.result_codes?.transaction ||
      data.extras?.result_codes?.operations?.join(', ') ||
      data.title ||
      'Transaction failed';
    return { hash: '', success: false, error };
  }
}
