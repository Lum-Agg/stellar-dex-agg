import './style.css';
import { LumAggClient } from '@lumagg/sdk';
import {
  isConnected,
  requestAccess,
  signTransaction,
} from '@stellar/freighter-api';

const API_URL = import.meta.env.VITE_API_URL || 'https://api.lumagg.xyz';
const XLM = 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA';
const USDC = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';

const client = new LumAggClient({ apiUrl: API_URL });

const logEl = document.getElementById('log') as HTMLPreElement;
const addressEl = document.getElementById('address') as HTMLSpanElement;
const amountEl = document.getElementById('amount') as HTMLInputElement;
const dryRunEl = document.getElementById('dryRun') as HTMLInputElement;
const connectBtn = document.getElementById('connect') as HTMLButtonElement;
const swapBtn = document.getElementById('swap') as HTMLButtonElement;

let address = '';

function log(msg: string) {
  const line = `[${new Date().toISOString().slice(11, 19)}] ${msg}`;
  logEl.textContent = `${logEl.textContent || ''}${line}\n`;
  logEl.scrollTop = logEl.scrollHeight;
}

connectBtn.addEventListener('click', async () => {
  try {
    const connected = await isConnected();
    if (!connected.isConnected) {
      throw new Error('Freighter not detected — install the extension and reload');
    }
    const access = await requestAccess();
    if (access.error) throw new Error(String(access.error));
    address = String(access.address || '');
    if (!address.startsWith('G')) throw new Error('No Freighter address returned');
    addressEl.textContent = address;
    swapBtn.disabled = false;
    log(`Connected ${address}`);
  } catch (e) {
    log(`Connect failed: ${e instanceof Error ? e.message : String(e)}`);
  }
});

swapBtn.addEventListener('click', async () => {
  if (!address) return;
  swapBtn.disabled = true;
  try {
    const amountIn = amountEl.value.trim() || '10000000';
    log(`API ${API_URL}`);
    log(`Checking XLM balance…`);
    const xlmBal = await client.getBalance({ account: address, token: XLM });
    log(`XLM balance=${xlmBal.balance ?? '?'} trustline=${xlmBal.hasTrustline}`);

    const usdcBal = await client.getBalance({ account: address, token: USDC });
    if (usdcBal.hasTrustline === false) {
      throw new Error('USDC trustline missing — add USDC in Freighter first (~0.5 XLM reserve)');
    }
    log(`USDC has_trustline=${usdcBal.hasTrustline} balance=${usdcBal.balance ?? '0'}`);

    log(`Quoting ${amountIn} stroops XLM → USDC (preferSoroban)…`);
    const { quote, tx } = await client.quoteAndBuild({
      tokenIn: XLM,
      tokenOut: USDC,
      amountIn,
      slippage: 0.5,
      preferSoroban: true,
      userPublicKey: address,
    });
    log(
      `Quote expected_out=${quote.expectedOutput} min_out=${quote.minimumOutput} legs=${quote.subRoutes.length}`,
    );
    log(`build_tx execution=${tx.execution} fee=${tx.fee}`);

    log('Requesting Freighter signature…');
    const signed = await signTransaction(tx.unsignedTxXdr, {
      networkPassphrase: 'Public Global Stellar Network ; September 2015',
      address,
    });
    if (signed.error) throw new Error(String(signed.error));
    const signedXdr = String(signed.signedTxXdr || '');
    if (!signedXdr) throw new Error('Freighter returned empty signedTxXdr');
    log(`Signed XDR length=${signedXdr.length}`);

    if (dryRunEl.checked) {
      log('Dry-run: stopping before submit_tx');
      return;
    }

    log('Submitting via LumAgg /submit_tx…');
    const submitted = await client.submitTx({ signedTxXdr: signedXdr });
    log(`Submitted hash=${submitted.hash} status=${submitted.status ?? ''}`);

    log('Waiting for confirmation…');
    const status = await client.waitForTx(submitted.hash);
    log(
      `Final confirmed=${status.confirmed} status=${status.status ?? ''} error=${status.error ?? ''}`,
    );
  } catch (e) {
    log(`Swap failed: ${e instanceof Error ? e.message : String(e)}`);
  } finally {
    swapBtn.disabled = !address;
  }
});

log(`Ready. API=${API_URL}`);
