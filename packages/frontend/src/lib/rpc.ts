/**
 * Frontend helpers for signing and submitting through Stellar Soroban RPC.
 * LumAgg's submit endpoint remains available as an optional fallback.
 */

import { Networks, TransactionBuilder, rpc } from '@stellar/stellar-sdk';
import { fetchTokenBalance } from '@/lib/balance';

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
  /** Default: preference from localStorage, else official Soroban RPC. */
  via?: SubmitVia;
  /** Used when via=official. Default: public (mainnet). */
  network?: SubmitNetwork;
}

export function getSubmitViaPreference(): SubmitVia {
  if (typeof window === 'undefined') return 'official';
  try {
    return localStorage.getItem(SUBMIT_VIA_STORAGE_KEY) === 'lumagg' ? 'lumagg' : 'official';
  } catch {
    return 'official';
  }
}

export function setSubmitViaPreference(via: SubmitVia): void {
  if (typeof window === 'undefined') return;
  try {
    if (via === 'lumagg') localStorage.setItem(SUBMIT_VIA_STORAGE_KEY, 'lumagg');
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

/** Fast enqueue only — clients should poll {@link waitForTxConfirmation}. */
async function submitViaOfficialRpc(
  signedXdr: string,
  network: SubmitNetwork,
): Promise<{ hash: string; success: boolean; error?: string; status?: string }> {
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
      status: response.status,
      error: 'Transaction rejected by the network',
    };
  }

  if (response.status === 'PENDING' || response.status === 'DUPLICATE') {
    return { hash: response.hash, success: true, status: response.status };
  }

  return {
    hash: response.hash ?? '',
    success: false,
    status: response.status,
    error: `Unexpected RPC status: ${response.status}`,
  };
}

async function submitViaApiServer(
  signedXdr: string,
  apiUrl?: string,
): Promise<{ hash: string; success: boolean; error?: string; status?: string }> {
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
  const hash = data.hash || '';
  if (data.success && !hash) {
    return {
      hash: '',
      success: false,
      status: data.status,
      error: 'Transaction was accepted without a transaction hash',
    };
  }
  return {
    hash,
    success: !!data.success,
    status: data.status,
    error: data.error,
  };
}

export async function submitSignedTransaction(
  signedXdr: string,
  opts?: SubmitTxOptions,
): Promise<{ hash: string; success: boolean; error?: string; status?: string }> {
  const via = opts?.via ?? getSubmitViaPreference();
  if (via === 'official') {
    return submitViaOfficialRpc(signedXdr, opts?.network ?? 'public');
  }
  return submitViaApiServer(signedXdr, opts?.apiUrl);
}

export type TxStatus = {
  success: boolean;
  hash?: string;
  status?: string;
  confirmed: boolean;
  error?: string;
};

export type FetchTxStatusOptions = {
  apiUrl?: string;
  /** Default: same preference as submit (Advanced toggle). */
  via?: SubmitVia;
  network?: SubmitNetwork;
};

async function fetchTxStatusViaOfficialRpc(
  hash: string,
  network: SubmitNetwork,
): Promise<TxStatus> {
  const rpcUrl = network === 'testnet' ? OFFICIAL_TESTNET_RPC_URL : OFFICIAL_MAINNET_RPC_URL;
  const server = new rpc.Server(rpcUrl);
  try {
    const result = await server.getTransaction(hash);
    const raw = String(result.status);
    const status =
      result.status === rpc.Api.GetTransactionStatus.SUCCESS
        ? 'SUCCESS'
        : result.status === rpc.Api.GetTransactionStatus.FAILED
          ? 'FAILED'
          : result.status === rpc.Api.GetTransactionStatus.NOT_FOUND
            ? 'NOT_FOUND'
            : raw;
    return {
      success: true,
      hash,
      status,
      confirmed: status === 'SUCCESS',
      error: status === 'FAILED' ? 'Transaction failed on-chain' : undefined,
    };
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : 'getTransaction failed';
    return { success: false, hash, confirmed: false, error: message };
  }
}

async function fetchTxStatusViaApi(hash: string, apiUrl?: string): Promise<TxStatus> {
  const base = apiUrl?.trim() || API_URL;
  const params = new URLSearchParams({ hash });
  const resp = await fetch(`${base}/api/v1/tx_status?${params}`);
  const data = (await resp.json()) as TxStatus;
  return {
    success: !!data.success,
    hash: data.hash,
    status: data.status,
    confirmed: !!data.confirmed,
    error: data.error,
  };
}

/** One-shot status check — official RPC or LumAgg API (matches submit path). */
export async function fetchTxStatus(
  hash: string,
  opts?: FetchTxStatusOptions | string,
): Promise<TxStatus> {
  // Back-compat: fetchTxStatus(hash, apiUrl)
  const options: FetchTxStatusOptions =
    typeof opts === 'string' ? { apiUrl: opts } : (opts ?? {});
  const via = options.via ?? getSubmitViaPreference();
  if (via === 'official') {
    return fetchTxStatusViaOfficialRpc(hash, options.network ?? 'public');
  }
  return fetchTxStatusViaApi(hash, options.apiUrl);
}

export type WaitForTxOptions = {
  apiUrl?: string;
  via?: SubmitVia;
  network?: SubmitNetwork;
  /** Max wait (ms). Default 60s. */
  timeoutMs?: number;
  /** Poll interval (ms). Default 1s. */
  intervalMs?: number;
  /**
   * For ChangeTrust: also succeed when SAC balance reports has_trustline.
   * Useful when getTransaction lags but classic trustline is already live.
   */
  trustline?: { account: string; token: string };
};

/**
 * After fast submit, poll until SUCCESS / FAILED / timeout.
 * Status source follows Advanced toggle (official RPC vs `/api/v1/tx_status`).
 * Optionally short-circuit when trustline appears on `/api/v1/balance`.
 */
export async function waitForTxConfirmation(
  hash: string,
  opts?: WaitForTxOptions,
): Promise<{ success: boolean; status?: string; error?: string }> {
  if (!hash) {
    return { success: false, error: 'Missing transaction hash' };
  }

  const timeoutMs = opts?.timeoutMs ?? 60_000;
  const intervalMs = opts?.intervalMs ?? 1_000;
  const deadline = Date.now() + timeoutMs;
  const via = opts?.via ?? getSubmitViaPreference();
  const network = opts?.network ?? 'public';
  let lastStatusError: string | undefined;

  while (Date.now() < deadline) {
    if (opts?.trustline) {
      const bal = await fetchTokenBalance(opts.trustline.account, opts.trustline.token);
      if (bal?.hasTrustline === true) {
        return { success: true, status: 'TRUSTLINE_READY' };
      }
    }

    try {
      const tx = await fetchTxStatus(hash, {
        apiUrl: opts?.apiUrl,
        via,
        network,
      });
      if (tx.confirmed || tx.status === 'SUCCESS') {
        return { success: true, status: tx.status ?? 'SUCCESS' };
      }
      if (tx.status === 'FAILED') {
        return {
          success: false,
          status: 'FAILED',
          error: tx.error || 'Transaction failed on-chain',
        };
      }
      if (!tx.success && tx.error) lastStatusError = tx.error;
    } catch (err) {
      lastStatusError = err instanceof Error ? err.message : 'Transaction status unavailable';
    }

    await sleep(intervalMs);
  }

  return {
    success: false,
    status: 'TIMEOUT',
    error: `Transaction submitted but not confirmed within ${Math.ceil(timeoutMs / 1000)}s — check the hash on stellar.expert${
      lastStatusError ? ` (last status error: ${lastStatusError})` : ''
    }`,
  };
}
