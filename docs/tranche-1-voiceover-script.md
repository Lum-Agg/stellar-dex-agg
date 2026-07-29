# LumAgg Tranche 1 voice-over script

This script follows the demo order: D3, D2, D1, then D4. Read slowly and pause
between sentences. Text in Chinese brackets is an action cue and should not be
read aloud.

## Pronunciation

- LumAgg: **Loom Agg**
- Tranche: **trahnsh**
- DEX: **decks**
- API: say **A P I**
- XDR: say **X D R**
- CLMM: say **C L M M**
- Soroban: **So-ro-ban**
- Soroswap: **So-ro-swap**
- Aquarius: **A-quare-ee-us**

## 0:00 - Introduction

[Show the title card, then open lumagg.xyz.]

Hello. This is LumAgg, a DEX aggregator on Stellar.

Tranche One has four deliverables: the swap interface, public API, benchmark,
and analytics indexer. I will show the live product and evidence.

## 0:25 - D3: Swap interface

[Open the token selector and select XLM to USDC.]

Deliverable Three is the swap interface on Stellar mainnet. The token selector
shows names and logos. 
After connecting, it shows your latest activities and spendable balances.

The quick buttons select twenty-five, fifty, seventy-five, or one hundred
percent.

[Open settings and request a quote.]

Users can change slippage, maximum hops, and maximum splits. Maximum hops
limits the route length. Maximum splits limits the number of parallel routes.
The route preview shows each DEX and its percentage. A successful transaction
provides a Stellar Expert link.

I will not submit a new transaction during this recording.

## 1:25 - D2: Integrator-ready API

[Open lumagg.xyz/docs.]

Deliverable Two is the public API. Its documentation covers quote, build
transaction, tokens, balances, health, API keys, and rate limits.

Integrators can select Soroban-only routes.

[Open the committed external evidence folder.]

A tester used LumAgg's public API to test the quote and build transaction
endpoints. The test results are included here.

## 2:55 - D1: Benchmark and comparison

[Open the reviewer summary at the top of scf-benchmark-results.md.]

Deliverable One is the benchmark comparison. It covers three trading pairs and
at least three trade sizes.

The fair Soroban-only rows show LumAgg output close to Soroswap.

It also includes split-routing examples and Aquarius C L M M routes.

[Open scf-venue-comparison.md briefly.]

Compared with Stellar Broker's public router and Soroswap, LumAgg covers more
Stellar liquidity venues, including Aquarius CLMM and Sushi CLMM.

## 3:50 - D4: Analytics indexer

[Open lumagg.xyz/stats.]

Deliverable Four is the analytics indexer. It reads LumAgg events from Stellar
mainnet and records transactions, notional, routed volume, users, functions, and
DEX legs.

The page shows daily activity routed through LumAgg to Stellar DEX venues.

The same data is available through public JSON and CSV endpoints. Indexing will
continue for the longer report in a later tranche.

## 4:40 - Closing

[Show the final card or repository root.]

This completes LumAgg Tranche One. The product, evidence, and verification links are public.

Thank you for reviewing LumAgg.



Production application: https://lumagg.xyz
API health: https://api.lumagg.xyz/api/v1/health
API documentation: https://lumagg.xyz/docs
Public repository: https://github.com/Lum-Agg/stellar-dex-agg
Benchmark results: https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/scf-benchmark-results.md
Venue comparison: https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/scf-venue-comparison.md
Reproducible benchmark script: https://github.com/Lum-Agg/stellar-dex-agg/blob/main/scripts/scf-benchmark.sh
Integrator guide: https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/integrator-guide.md
OpenAPI specification: https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/openapi.yaml
External integrator evidence: https://github.com/Lum-Agg/stellar-dex-agg/tree/main/docs/evidence/d2-integrator-smoke
Public stats page: https://lumagg.xyz/stats
Stats JSON/CSV API: https://api.lumagg.xyz/api/v1/stats
Analytics attribution and pipeline: https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/analytics-indexer.md
Sample indexer export: https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/sample-indexer-export.json
Mainnet Aggregator contract: https://stellar.expert/explorer/public/contract/CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K
Mainnet three-path split execution: https://stellar.expert/explorer/public/tx/a571b4617bc42594673ab22a496ef61c4fc66689a4f9cc29fd71dc7fb74ccb54