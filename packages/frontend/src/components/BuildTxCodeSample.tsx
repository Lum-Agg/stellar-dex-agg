'use client';

import { useState } from 'react';

const AGGREGATOR = 'CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K';
const NETWORK_PASSPHRASE = 'Public Global Stellar Network ; September 2015';

function buildSampleCode(apiUrl: string, rpcUrl: string): string {
  return `// 本地组装交易（不调用 POST /api/v1/build_tx）
// 依赖: npm install @stellar/stellar-sdk
import {
  Contract,
  Address,
  TransactionBuilder,
  Horizon,
  rpc,
  BASE_FEE,
} from '@stellar/stellar-sdk';

const API_URL = '${apiUrl}';
const RPC_URL = '${rpcUrl}'; // Soroban RPC（仅用于 simulate，非 build_tx）
const AGGREGATOR = '${AGGREGATOR}';
const NETWORK_PASSPHRASE = '${NETWORK_PASSPHRASE}';

const TOKEN_IN = 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA';
const TOKEN_OUT = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';

// quote.dex_types → 合约枚举名（与链上一致）
const DEX_ENUM: Record<string, string> = {
  aquarius: 'Aquarius',
  aquarius_clmm: 'Aquarius',
  soroswap: 'SoroswapPair',
  phoenix: 'Phoenix',
  sushi: 'Sushi',
  comet: 'CometDex',
};

/** 把 /quote 的 sub_routes 转成 aggregator.swap 的 SubRoute[] */
function quoteToContractSubRoutes(quoteSubRoutes: Array<{
  amount_in: string;
  path: string[];
  pool_addresses: string[];
  dex_types: string[];
  in_indices: number[];
  out_indices: number[];
}>) {
  return quoteSubRoutes.map((leg) => ({
    amount_in: BigInt(leg.amount_in),
    steps: leg.pool_addresses.map((pool, i) => ({
      dex_id: pool,
      dex_type: DEX_ENUM[leg.dex_types[i]] ?? leg.dex_types[i],
      token_in: leg.path[i],
      token_out: leg.path[i + 1],
      in_idx: leg.in_indices[i] ?? 0,
      out_idx: leg.out_indices[i] ?? 1,
    })),
  }));
}

async function swapWithLocalTx(userSecret: string) {
  const server = new Horizon.Server('https://horizon.stellar.org');
  const soroban = new rpc.Server(RPC_URL);
  const source = await server.loadAccount(Address.fromSecret(userSecret).accountId());

  const amountIn = '10000000'; // 1 XLM

  // 1) 只向 LumAgg 要路由（quote）
  const q = await fetch(
    \`\${API_URL}/api/v1/quote?token_in=\${TOKEN_IN}&token_out=\${TOKEN_OUT}&amount_in=\${amountIn}&slippage=0.5\`
  );
  const { data: quote } = await (await q.json());
  const subRoutes = quoteToContractSubRoutes(quote.sub_routes);

  // 2) 本地构造 invoke：aggregator.swap(user, token_in, token_out, sub_routes, min_out)
  const contract = new Contract(AGGREGATOR);
  const user = Address.fromSecret(userSecret);
  const tokenIn = Address.contract(TOKEN_IN);
  const tokenOut = Address.contract(TOKEN_OUT);

  let tx = new TransactionBuilder(source, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(
      contract.call(
        'swap',
        user,
        tokenIn,
        tokenOut,
        subRoutes,
        BigInt(quote.minimum_output),
      ),
    )
    .setTimeout(300)
    .build();

  // 3) Soroban 仍需 RPC simulate（填充 footprint / auth），但不是我们的 build_tx
  const sim = await soroban.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(sim)) throw new Error(sim.error);
  const prepared = rpc.assembleTransaction(tx, sim).build();

  // 4) 签名并提交
  prepared.sign(user);
  await server.submitTransaction(prepared);
}

// 注意：
// - 若 quote 含 classic_dex，需改用 PathPaymentStrictSend，不能走 aggregator.swap
// - 单条 leg 内不要混 classic 与 Soroban hop`;
}

export function BuildTxCodeSample({
  apiUrl,
  rpcUrl = 'https://soroban-rpc.mainnet.stellar.gateway.fm',
}: {
  apiUrl: string;
  rpcUrl?: string;
}) {
  const [show, setShow] = useState(false);
  const code = buildSampleCode(apiUrl, rpcUrl);

  return (
    <div className="mb-4">
      <div className="flex flex-wrap items-center justify-between gap-2 mb-2">
        <h4 className="text-xs font-bold text-gray-500 uppercase">本地组装交易</h4>
        <button
          type="button"
          onClick={() => setShow((v) => !v)}
          className="px-3 py-1 rounded-md text-xs font-medium border border-white/15 bg-white/5 hover:bg-white/10 text-slate-200 transition-colors"
        >
          {show ? '隐藏代码' : '展示代码'}
        </button>
      </div>
      <p className="text-xs text-gray-500 mb-2 leading-relaxed">
        只调用 <code className="text-blue-300/90">GET /quote</code> 拿路由；在你自己的服务里用{' '}
        <code className="text-blue-300/90">@stellar/stellar-sdk</code> 组装{' '}
        <code className="text-blue-300/90">aggregator.swap</code>。不需要{' '}
        <code className="text-blue-300/90">POST /build_tx</code>，但 Soroban 仍要对任意 RPC 做{' '}
        <code className="text-blue-300/90">simulateTransaction</code>（与是否用 LumAgg build_tx 无关）。
      </p>
      {show && (
        <pre className="bg-black/70 rounded-lg p-4 text-[11px] leading-relaxed text-slate-300 overflow-x-auto border border-white/10 font-mono whitespace-pre">
          {code}
        </pre>
      )}
    </div>
  );
}
