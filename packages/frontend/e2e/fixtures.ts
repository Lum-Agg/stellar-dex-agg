import { test as base, expect } from '@playwright/test';
import {
  Account,
  Asset,
  Networks,
  Operation,
  StrKey,
  TransactionBuilder,
} from '@stellar/stellar-sdk/minimal';

export const XLM = 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA';
export const USDC = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';
export const CUSTOM = `C${'B'.repeat(55)}`;
export const ADDRESS = StrKey.encodeEd25519PublicKey(Buffer.alloc(32, 7));
const transaction = new TransactionBuilder(new Account(ADDRESS, '1'), {
  fee: '100',
  networkPassphrase: Networks.PUBLIC,
})
  .addOperation(Operation.payment({ destination: ADDRESS, asset: Asset.native(), amount: '1' }))
  .setTimeout(60)
  .build();
export const HASH = transaction.hash().toString('hex');
const XDR = transaction.toXDR();

declare global {
  interface Window {
    __mockWallet: { reject: boolean; signCalls: number; requests: string[] };
  }
}

export interface MockApp {
  balance: string;
  status: 'SUCCESS' | 'FAILED' | 'NOT_FOUND';
  statusErrors: number;
  submitResponse: 'accepted' | 'lost' | 'missing-hash';
  balanceCalls: number;
  statusCalls: number;
  rpcCalls: number;
  rpcSubmissions: number;
  builds: Record<string, unknown>[];
  submissions: Record<string, unknown>[];
  unexpected: string[];
}

export const test = base.extend<{ app: MockApp; connected: boolean }>({
  connected: [true, { option: true }],
  app: [
    async ({ page, context, connected }, use) => {
      const app: MockApp = {
        balance: '1000000000',
        status: 'SUCCESS',
        statusErrors: 0,
        submitResponse: 'accepted',
        balanceCalls: 0,
        statusCalls: 0,
        rpcCalls: 0,
        rpcSubmissions: 0,
        builds: [],
        submissions: [],
        unexpected: [],
      };
      const errors: string[] = [];
      page.on('pageerror', (error) => errors.push(error.message));
      await page.addInitScript(
        ({ address, connected }) => {
          if (!localStorage.getItem('e2e.initialized')) {
            localStorage.setItem('e2e.initialized', '1');
            localStorage.setItem('lumagg-theme', 'dark');
            localStorage.setItem('lumagg.submitViaOfficialRpc', 'lumagg');
            if (connected) {
              localStorage.setItem('@StellarWalletsKit/activeAddress', address);
              localStorage.setItem('@StellarWalletsKit/selectedModuleId', 'freighter');
            }
          }
          window.__mockWallet = { reject: false, signCalls: 0, requests: [] };
          Object.defineProperty(window, 'freighter', { value: true });
          window.addEventListener('message', (event) => {
            const request = event.data;
            if (event.source !== window || request?.source !== 'FREIGHTER_EXTERNAL_MSG_REQUEST')
              return;
            window.__mockWallet.requests.push(request.type);
            const signing = request.type === 'SUBMIT_TRANSACTION';
            if (signing) window.__mockWallet.signCalls++;
            window.postMessage(
              {
                source: 'FREIGHTER_EXTERNAL_MSG_RESPONSE',
                messagedId: request.messageId,
                publicKey: address,
                isConnected: true,
                isAllowed: true,
                signerAddress: address,
                signedTransaction: request.transactionXdr,
                networkDetails: {
                  network: 'PUBLIC',
                  networkPassphrase: 'Public Global Stellar Network ; September 2015',
                },
                apiError:
                  signing && window.__mockWallet.reject
                    ? { code: -1, message: 'User rejected transaction' }
                    : undefined,
              },
              window.location.origin,
            );
          });
        },
        { address: ADDRESS, connected },
      );

      await context.route('**/*', async (route) => {
        const request = route.request();
        const url = new URL(request.url());
        if (url.origin === 'http://127.0.0.1:3105') return route.continue();
        if (request.resourceType() === 'image')
          return route.fulfill({
            contentType: 'image/svg+xml',
            body: '<svg xmlns="http://www.w3.org/2000/svg"/>',
          });
        const cors = {
          'access-control-allow-origin': '*',
          'access-control-allow-methods': 'GET,POST,OPTIONS',
          'access-control-allow-headers': '*',
        };
        if (request.method() === 'OPTIONS') return route.fulfill({ status: 204, headers: cors });
        const json = (data: unknown, status = 200) =>
          route.fulfill({
            status,
            headers: cors,
            contentType: 'application/json',
            body: JSON.stringify(data),
          });
        if (url.origin === 'https://mainnet.sorobanrpc.com') {
          app.rpcCalls++;
          const body = request.postDataJSON();
          if (body.method === 'getTransaction')
            return json({
              jsonrpc: '2.0',
              id: body.id,
              result: { status: app.status, resultXdr: 'unsupported-future-xdr' },
            });
          if (body.method === 'sendTransaction') {
            app.rpcSubmissions++;
            if (app.submitResponse === 'lost') return route.abort('failed');
            return json({
              jsonrpc: '2.0',
              id: body.id,
              result: {
                status: 'PENDING',
                hash: HASH,
                latestLedger: 100,
                latestLedgerCloseTime: 1000,
              },
            });
          }
          app.unexpected.push(`RPC ${body.method}`);
          return json({ error: 'Unexpected RPC request' }, 500);
        }
        if (url.origin === 'https://lumagg-api.test') {
          switch (url.pathname) {
            case '/api/v1/tokens':
              return json({
                tokens: [
                  { id: XLM, symbol: 'XLM', name: 'Stellar Lumens' },
                  { id: USDC, symbol: 'USDC', name: 'USD Coin' },
                  { id: CUSTOM, symbol: 'TEST', name: 'E2E Test Token' },
                ],
              });
            case '/api/v1/balances':
              app.balanceCalls++;
              return json({
                success: true,
                scope: url.searchParams.get('scope'),
                tokens_queried: [XLM, USDC, CUSTOM],
                balances: { [XLM]: app.balance, [USDC]: '500000000', [CUSTOM]: '100000000' },
                has_trustline: { [XLM]: true, [USDC]: true, [CUSTOM]: true },
              });
            case '/api/v1/balance':
              return json({ success: true, balance: '1000000000', has_trustline: true });
            case '/api/v1/quote': {
              const input = url.searchParams.get('amount_in')!;
              const output = (BigInt(input) / BigInt(5)).toString();
              return json({
                success: true,
                data: {
                  amount_in: input,
                  expected_output: output,
                  minimum_output: ((BigInt(output) * BigInt(995)) / BigInt(1000)).toString(),
                  price_impact: 0.01,
                  is_split: false,
                  compute_time_ms: 1,
                  sub_routes: [
                    {
                      source: 'aquarius',
                      path: [url.searchParams.get('token_in'), url.searchParams.get('token_out')],
                      pool_addresses: ['MOCK_POOL'],
                      dex_types: ['aquarius'],
                      in_indices: [0],
                      out_indices: [1],
                      amount_in: input,
                      amount_out: output,
                      percentage: 100,
                    },
                  ],
                },
              });
            }
            case '/api/v1/build_tx':
              app.builds.push(request.postDataJSON());
              return json({ success: true, data: { unsigned_tx_xdr: XDR } });
            case '/api/v1/submit_tx':
              app.submissions.push(request.postDataJSON());
              if (app.submitResponse === 'lost') return route.abort('failed');
              if (app.submitResponse === 'missing-hash')
                return json({ success: true, status: 'PENDING' });
              return json({ success: true, hash: HASH, status: 'PENDING' });
            case '/api/v1/tx_status':
              app.statusCalls++;
              if (app.statusErrors-- > 0) return json({ error: 'Temporarily unavailable' }, 503);
              return json({
                success: true,
                hash: HASH,
                status: app.status,
                confirmed: app.status === 'SUCCESS',
                error: app.status === 'FAILED' ? 'Transaction failed on-chain' : undefined,
              });
            case '/api/v1/swaps':
              return json({ success: true, data: { swaps: [], next_cursor: null } });
          }
        }
        app.unexpected.push(request.url());
        return route.abort('blockedbyclient');
      });
      await context.routeWebSocket('**', (socket) => {
        app.unexpected.push(`WebSocket ${socket.url()}`);
        socket.close();
      });
      await use(app);
      expect(app.unexpected, 'No unmocked API/RPC requests may escape to the network').toEqual([]);
      expect(errors, 'No unhandled browser exceptions').toEqual([]);
    },
    { auto: true },
  ],
});

export { expect };
