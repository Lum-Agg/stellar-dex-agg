/**
 * Frontend helpers that proxy Soroban RPC calls through api-server by default.
 * Optional advanced path: submit via Stellar official RPC (whitelist only).
 */

import { Networks, TransactionBuilder, rpc } from '@stellar/stellar-sdk';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';
const LIMIT_API_URL = process.env.NEXT_PUBLIC_LIMIT_API_URL?.trim() || '';

/** Whitelisted official RPCs (not user-editable). */
export const OFFICIAL_MAINNET_RPC_URL = 'https://mainnet.sorobanrpc.com';
export const OFFICIAL_TESTNET_RPC_URL = 'https://soroban-testnet.stellar.org';

const SUBMIT_VIA_STORAGE_KEY = 'lumagg.submitViaOfficialRpc';

export type SubmitVia = 'lumagg' | 'official';
export type SubmitNetwork = 'public' | 'testnet';

export interface SubmitTxOptions {
  apiUrl?: string;
  /** Default: preference from localStorage, else lumagg api-server. */
  via?: SubmitVia;
  /** Used when via=official. Default: public (mainnet). */
  network?: SubmitNetwork;
}

export function getSubmitViaPreference(): SubmitVia {
  if (typeof window === 'undefined') return 'lumagg';
  try {
    return localStorage.getItem(SUBMIT_VIA_STORAGE_KEY) === '1' ? 'official' : 'lumagg';
  } catch {
    return 'lumagg';
  }
}

export function setSubmitViaPreference(via: SubmitVia): void {
  if (typeof window === 'undefined') return;
  try {
    if (via === 'official') localStorage.setItem(SUBMIT_VIA_STORAGE_KEY, '1');
    else localStorage.removeItem(SUBMIT_VIA_STORAGE_KEY);
  } catch {
    /* ignore quota / private mode */
  }
}

export async function fetchAccountSequence(accountId: string): Promise<string> {
  const params = new URLSearchParams({ account: accountId });
  const resp = await fetch(`${API_URL}/api/v1/account?${params}`);
  const data = (await resp.json()) as { success?: boolean; sequence?: string; error?: string };
  if (!resp.ok || !data.success || !data.sequence) {
    throw new Error(data.error || 'Failed to fetch account sequence');
  }
  return data.sequence;
}

export async function fetchLatestLedger(apiUrl?: string): Promise<number> {
  const base = apiUrl?.trim() || LIMIT_API_URL || API_URL;
  const resp = await fetch(`${base}/api/v1/ledger/latest`);
  const data = (await resp.json()) as { success?: boolean; sequence?: number; error?: string };
  if (!resp.ok || !data.success || typeof data.sequence !== 'number' || data.sequence <= 0) {
    throw new Error(data.error || 'Failed to fetch latest ledger');
  }
  return data.sequence;
}

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function submitViaOfficialRpc(
  signedXdr: string,
  network: SubmitNetwork,
): Promise<{ hash: string; success: boolean; error?: string }> {
  const isTestnet = network === 'testnet';
  const networkPassphrase = isTestnet ? Networks.TESTNET : Networks.PUBLIC;
  const rpcUrl = isTestnet ? OFFICIAL_TESTNET_RPC_URL : OFFICIAL_MAINNET_RPC_URL;
  const tx = TransactionBuilder.fromXDR(signedXdr, networkPassphrase);
  const server = new rpc.Server(rpcUrl);

  let response = await server.sendTransaction(tx);
  let attempts = 0;
  while (response.status === 'TRY_AGAIN_LATER' && attempts < 30) {
    await sleep(1000);
    response = await server.sendTransaction(tx);
    attempts += 1;
  }

  if (response.status === 'ERROR') {
    return {
      hash: response.hash,
      success: false,
      error: 'Transaction rejected by the network',
    };
  }

  if (response.status === 'PENDING' || response.status === 'DUPLICATE') {
    return { hash: response.hash, success: true };
  }

  return {
    hash: response.hash ?? '',
    success: false,
    error: `Unexpected RPC status: ${response.status}`,
  };
}

async function submitViaApiServer(
  signedXdr: string,
  apiUrl?: string,
): Promise<{ hash: string; success: boolean; error?: string }> {
  const base = apiUrl?.trim() || API_URL;
  const resp = await fetch(`${base}/api/v1/submit_tx`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ signed_tx_xdr: signedXdr }),
  });
  const data = (await resp.json()) as {
    success?: boolean;
    hash?: string;
    error?: string;
    status?: string;
  };
  return {
    hash: data.hash || '',
    success: !!data.success,
    error: data.error,
  };
}

export async function submitSignedTransaction(
  signedXdr: string,
  opts?: SubmitTxOptions,
): Promise<{ hash: string; success: boolean; error?: string }> {
  const via = opts?.via ?? getSubmitViaPreference();
  if (via === 'official') {
    return submitViaOfficialRpc(signedXdr, opts?.network ?? 'public');
  }
  return submitViaApiServer(signedXdr, opts?.apiUrl);
}
