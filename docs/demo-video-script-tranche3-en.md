# LumAgg SCF #44 Tranche 3 Demo Script

Target duration: approximately five minutes. Record the screen with voice-over, upload the video as Unlisted, and provide a publicly playable URL in the Tranche 3 completion form.

## Pre-recording setup

Open these pages before recording:

- `https://lumagg.xyz/`
- `https://lumagg.xyz/docs`
- `https://lumagg.xyz/stats`
- `https://api.lumagg.xyz/api/v1/health`
- `https://github.com/Lum-Agg/stellar-dex-agg`

Do not show private keys, seed phrases, server passwords, Telegram tokens, or `.env` files. Signing a real transaction is optional; showing a quote and unsigned XDR is sufficient.

## 0:00–0:25 — Introduction

**Show:** The LumAgg homepage, starting on the Swap page.

**Say:**

> This is LumAgg, an open-source DEX aggregator and atomic arbitrage infrastructure for Stellar. In this Tranche 3 demonstration, I will show the public analytics, integrator API and SDK, the production swap and arbitrage stack, the self-hosting documentation, and the smart-contract audit scope.

## 0:25–1:10 — Swap product

**Actions:**

1. Select `XLM` as the sell token and `USDC` as the buy token.
2. Show the token logos, wallet balance, and 25%, 50%, 75%, and Max amount buttons.
3. Open Swap Settings and show slippage, maximum hops, and maximum splits.
4. Enter a small amount and request a quote.
5. Expand the route and show the DEX names, path, and split legs when available.
6. Optionally continue to the wallet signing screen. Do not send a real transaction unless desired.

**Say:**

> This is the LumAgg user-facing swap flow. Users can see their wallet balance, select an amount, configure slippage, and inspect the route selected by the aggregator. A quote may use one route or split the trade across multiple DEXes. Before signing, the interface shows the expected output and minimum received amount. The user always confirms the final transaction in their wallet.

## 1:10–1:55 — API and TypeScript SDK

**Show:** `https://lumagg.xyz/docs`, then switch to a terminal.

**Run:**

```bash
curl -sS 'https://api.lumagg.xyz/api/v1/health' | jq

curl -sG 'https://api.lumagg.xyz/api/v1/quote' \
  --data-urlencode 'token_in=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA' \
  --data-urlencode 'token_out=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75' \
  --data-urlencode 'amount_in=10000000' \
  --data-urlencode 'prefer_soroban=1' \
  --data-urlencode 'max_splits=2' | jq
```

**Point out:** `success`, `expected_output`, `is_split`, `sub_routes`, DEX names, and pool addresses.

**Say:**

> LumAgg provides a public REST API, OpenAPI documentation, and a TypeScript SDK. An integrator can request a quote and then request `build_tx` to receive an unsigned XDR for the user to sign. The `prefer_soroban=1` option requests Soroban-only routing. The integration flow does not require the user to share a private key with LumAgg.

**Optional:** Show the npm package `@lumagg/sdk` and the SDK examples in the repository. Do not run examples that require a private key.

## 1:55–2:45 — Public analytics

**Show:** `https://lumagg.xyz/stats`. Display the overview, date range, transaction statistics, DEX breakdown, arbitrage statistics, and failure reasons.

**Run:**

```bash
curl -sS 'https://api.lumagg.xyz/api/v1/stats?format=csv' | head -5
```

**Say:**

> This is the production analytics dashboard. The data comes from indexed on-chain events, not from quotes or simulations. From July 13 through October 7, 2026, the index covers 86 days and records 9,473 aggregator invocations, 4,786 successful transactions, 13,727 DEX legs, and 4,700 round trips.
>
> The report keeps entry notional and routed DEX volume separate. Entry notional is the initial input amount, while routed volume sums the actual inputs of executed DEX legs. This avoids double-counting split routes. The frozen snapshot records approximately $77,521 in entry notional, $265,332 in routed DEX volume, and $227 in gross surplus. The same data is available as a CSV export for independent review.

## 2:45–3:35 — Arbitrage architecture and production evidence

**Show:** `docs/arb-operator.md`, then `docs/arb-evidence-snapshot.md` or the public arbitrage page.

**Say:**

> The arbitrage system consists of a scanner, callers, the Aggregator, and an Arb Vault. The Vault holds the arbitrage principal. An authorized caller invokes `execute_round_trip`, and the Aggregator executes an atomic round trip across Stellar DEXes. Execution is gated by caller authorization and minimum-profit checks. Results can be verified through on-chain transactions and the public analytics.
>
> User swaps and authorized arbitrage funds are separated. The Aggregator is non-custodial and does not hold ordinary user funds. The Arb Vault is restricted to authorized arbitrage execution.

**Show the contract IDs:**

- Mainnet Aggregator: `CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K`
- Mainnet Arb Vault: `CCQQ3LRFCSGOYSSD6S4MGH6RWWYVDHYPJO6KYDJYC2IDZK4OGCK6P6KN`

Do not show any seed phrase, private key, or caller secret.

## 3:35–4:15 — Self-hosting

**Show:**

- `docs/self-hosted-aggregator-quickstart.md`
- `docs/arbitrage-deployment.md`
- `docs/deployment-overview.md`

**Say:**

> LumAgg is not limited to the hosted website. Developers can deploy their own Aggregator API, market-data worker, Redis, and Stellar RPC or Horizon infrastructure. They can also run the arbitrage bot separately. The repository includes configuration, systemd deployment, health checks, and operational documentation. This lets integrators use the open-source code and SDK while retaining control over their own infrastructure, keys, and data sources.

## 4:15–4:45 — Audit scope and security boundary

**Show:** `docs/audit-scope.md` in the GitHub repository.

**Say:**

> The Tranche 3 third-party audit scope covers the Mainnet Aggregator and Arb Vault. The review focuses on authorization, DEX cross-contract calls, split swaps, round trips, caller allowlists, and fund flows.
>
> Order Escrow is currently deployed only on testnet. It is outside this audit scope and will remain testnet-only. A separate review is required for authorization, refunds, expiry reclaim, and cross-contract fund flows before any future Limit or DCA mainnet deployment.
>
> This preparation does not authorize a contract upgrade. Any critical or high-severity finding must be fixed, retested, and explicitly approved before an upgrade is considered.

## 4:45–5:00 — Closing

**Show:** The GitHub repository, then the LumAgg homepage or an end card.

**Say:**

> This concludes the LumAgg Tranche 3 demonstration: the production swap interface, public API and SDK, on-chain analytics, atomic arbitrage architecture, self-hosting materials, and auditable contract scope. The review links are the LumAgg website, public API, Stats dashboard, and GitHub repository. Thank you for watching.

**End card:**

```text
https://lumagg.xyz
https://lumagg.xyz/stats
https://api.lumagg.xyz
https://github.com/Lum-Agg/stellar-dex-agg
```

## Recording checklist

- Use the display's native resolution. Prefer a 16:9 browser window if practical, but do not upscale or force 1920×1080; readability is more important than the exact resolution. Hide bookmarks and private notifications.
- Preload one XLM → USDC quote so the live recording is quick.
- Run the terminal commands once before recording, then show only the relevant fields during the take.
- Use the frozen numbers from `docs/evidence/tranche3-stats.csv` and the final report; do not invent live metrics during the recording.
- If the Protocol 28 testnet regression is not complete, do not claim that all regression tests have passed. State that the checklist is being completed.
- After uploading, open the video in a private window to confirm that it plays without login, then paste the URL into the Tranche 3 completion form.

## TTS Voice-over Copy

The following section contains only the spoken narration. Copy this section into the TTS tool without the headings or formatting if needed.

```text
This is LumAgg, an open-source DEX aggregator and atomic arbitrage infrastructure for Stellar. In this Tranche 3 demonstration, I will show the public analytics, integrator API and SDK, the production swap and arbitrage stack, the self-hosting documentation, and the smart-contract audit scope.

This is the LumAgg user-facing swap flow. Users can see their wallet balance, select an amount, configure slippage, and inspect the route selected by the aggregator. A quote may use one route or split the trade across multiple DEXes. Before signing, the interface shows the expected output and minimum received amount. The user always confirms the final transaction in their wallet.

LumAgg provides a public REST API, OpenAPI documentation, and a TypeScript SDK. An integrator can request a quote and then request build transaction to receive an unsigned XDR for the user to sign. The prefer Soroban equals one option requests Soroban-only routing. The integration flow does not require the user to share a private key with LumAgg.

This is the production analytics dashboard. The data comes from indexed on-chain events, not from quotes or simulations. From July 13 through October 7, 2026, the index covers 86 days and records 9,473 aggregator invocations, 4,786 successful transactions, 13,727 DEX legs, and 4,700 round trips.

The report keeps entry notional and routed DEX volume separate. Entry notional is the initial input amount, while routed volume sums the actual inputs of executed DEX legs. This avoids double-counting split routes. The frozen snapshot records approximately 77,521 US dollars in entry notional, 265,332 US dollars in routed DEX volume, and 227 US dollars in gross surplus. The same data is available as a CSV export for independent review.

The arbitrage system consists of a scanner, callers, the Aggregator, and an Arb Vault. The Vault holds the arbitrage principal. An authorized caller invokes execute round trip, and the Aggregator executes an atomic round trip across Stellar DEXes. Execution is gated by caller authorization and minimum-profit checks. Results can be verified through on-chain transactions and the public analytics.

User swaps and authorized arbitrage funds are separated. The Aggregator is non-custodial and does not hold ordinary user funds. The Arb Vault is restricted to authorized arbitrage execution.

LumAgg is not limited to the hosted website. Developers can deploy their own Aggregator API, market-data worker, Redis, and Stellar RPC or Horizon infrastructure. They can also run the arbitrage bot separately. The repository includes configuration, systemd deployment, health checks, and operational documentation. This lets integrators use the open-source code and SDK while retaining control over their own infrastructure, keys, and data sources.

The Tranche 3 third-party audit scope covers the Mainnet Aggregator and Arb Vault. The review focuses on authorization, DEX cross-contract calls, split swaps, round trips, caller allowlists, and fund flows.

Order Escrow is currently deployed only on testnet. It is outside this audit scope and will remain testnet-only. A separate review is required for authorization, refunds, expiry reclaim, and cross-contract fund flows before any future Limit or DCA mainnet deployment.

This preparation does not authorize a contract upgrade. Any critical or high-severity finding must be fixed, retested, and explicitly approved before an upgrade is considered.

This concludes the LumAgg Tranche 3 demonstration: the production swap interface, public API and SDK, on-chain analytics, atomic arbitrage architecture, self-hosting materials, and auditable contract scope. The review links are the LumAgg website, public API, Stats dashboard, and GitHub repository. Thank you for watching.
```
