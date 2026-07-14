# SCF Build #44 — form copy (English draft)

Paste and adapt in the [SCF Dashboard](https://communityfund.stellar.org/) Build submission.

**Only items to customize:** your name / GitHub in **Team**, and XLM amount at submission time (budget is **USD-equivalent**).

---

## Project title

**LumAgg — Stellar DEX Aggregator**

## Short description (1–2 sentences)

LumAgg is an open-source liquidity aggregator for Stellar that finds the best swap price across Classic SDEX and Soroban AMMs (Aquarius, Soroswap, Phoenix, Sushi, Comet, CLMM) through a single API and web UI, with optional execution via a Soroban aggregator contract.

## Problem statement

Stellar liquidity is split across incompatible execution layers: Classic path payments vs Soroban contract swaps, each with different pool formats and discovery. Integrators must maintain many adapters, stale reserves, and fragile split-route logic. End users see inconsistent prices and failed transactions when routes are wrong or under-protected.

## Proposed solution

1. **market-data-worker** — Continuous pool discovery and reserve refresh; publishes normalized state to Redis (~2s cadence).
2. **api-server** — Stateless quote API: multi-hop path search, split optimization, `build_tx` for Classic (`PathPaymentStrictSend`) and Soroban (`aggregator.swap` + simulate).
3. **Soroban aggregator contract** — Bundles multi-leg Soroban swaps atomically on mainnet.
4. **Frontend** — [lumagg.xyz](https://lumagg.xyz) wallet-connected swap UI.

## What is already built (MVP) — not funded by this award

- Mainnet deployment: API (`api.lumagg.xyz`), worker, Redis pool cache
- 200+ xy=k pools, Aquarius + CLMM coverage, Classic DEX fallback
- Split routing with dust and rate sanity guards
- Classic `dest_min` compliance for PathPayment
- Public REST: `/api/v1/quote`, `/api/v1/build_tx`, `/api/v1/tokens`, `/api/v1/health`
- Demo / regression scripts: `./scripts/scf-demo.sh`, `./scripts/scf-quote-regression.sh`

This Build award funds **production hardening, integrations, and ecosystem readiness** on top of the live MVP.

## Funding request (summary)

> We request **$100,000 USD equivalent in XLM** over **~3.5 months** (three tranches: 1 mo + 1 mo + 1.5 mo), paid in **three tranches**, for a **solo full-time founder** to harden mainnet operations, ship integration-ready APIs/SDK and developer tooling, and complete the **Soroban Security Audit Bank** process for the aggregator contract. Full smart-contract audit fees are **not** duplicated in this budget; we will apply to the [Audit Bank](https://stellar.gitbook.io/scf-handbook/supporting-programs/audit-bank) after Build approval (budget includes co-pay reserve and remediation time only).

## Milestones & tranches

| Tranche | Amount (USD) | Timeline | Deliverables (acceptance criteria) |
|---------|--------------|----------|-----------------------------------|
| **T1** | $35,000 | Months 1–2 | Production SLOs for API + worker (uptime, pool publish freshness); public integration guide + OpenAPI; monitoring/alerting; CI running quote regression tests |
| **T2** | $35,000 | Months 3–4 | Comet/Sushi factory discovery fixes; published quote latency / pool coverage metrics; integration-ready interfaces and evidence (versioned API/SDK docs, reproducible integration harness, and sample app) |
| **T3** | $30,000 | Month 3–4.5 (~1.5 mo after T2) | Mainnet launch maturity + security-readiness; SDK/widget; public demo package; operational validation report (metrics + mainnet txs + regression logs); Audit Bank intake |

## Success metrics

- Quote latency P95 &lt; 500ms for typical trade sizes on cached pool state
- Pool state freshness: Redis publish interval ≤ 2s
- Zero fantasy split legs in regression suite (`scripts/scf-quote-regression.sh`)
- Successful end-to-end swaps on mainnet (Classic + Soroban) in public demo
- T2: integration-ready package completed (versioned docs + sample app + reproducible harness for `/api/v1/quote` and `/api/v1/build_tx`)
- T3: Audit Bank application completed; aggregator contract audit scheduled or in progress; final operational validation report (metrics, demo artifacts, example mainnet txs)

## Budget overview

| Category | Amount (USD) | Notes |
|----------|--------------|-------|
| Engineering (solo, full-time) | $72,000 | Router, adapters, API, UI, SDK — ~3.5 months |
| Infrastructure | $8,000 | Soroban RPC, Redis, servers, monitoring, domains (12 months runway) |
| Integrations & ecosystem | $10,000 | SDK/widget, integration docs, sample app and reproducible harness |
| Security (non-duplicative) | $5,000 | Audit Bank co-pay reserve + remediation labor (not full audit fee) |
| Community / ops | $5,000 | Demo video, Discord, grant reporting |

**Total requested:** **$100,000 USD equivalent in XLM** (convert at disbursement per SCF rules)

### Smart contract audit (separate program)

- Full audit cost: via **Soroban Security Audit Bank** after SCF Build approval ([handbook](https://stellar.gitbook.io/scf-handbook/supporting-programs/audit-bank)).
- SDF coordinates third-party firms (not an in-house SDF audit).
- Initial audit may require **5% co-pay** (refundable if critical/high/medium issues are fixed within 20 business days).
- This Build budget does **not** include a second $25k–$40k audit line item.

## Team

**[Your name]** — Founder & sole developer (full-time)

- Role: architecture, Rust backend (worker, router, API), Soroban contract maintenance, frontend, DevOps
- GitHub: [ligulfzhou](https://github.com/ligulfzhou) / repo: [stellar-dex-agg](https://github.com/ligulfzhou/stellar-dex-agg)
- Stellar: mainnet MVP deployed (api.lumagg.xyz, lumagg.xyz); SCF Interest Form accepted for Build #44

## Open source & license

- Repository: https://github.com/ligulfzhou/stellar-dex-agg
- License: [Apache-2.0](../LICENSE)

## Risks & mitigations

| Risk | Mitigation |
|------|------------|
| Stale pool state | Fast Redis publish + background reserve refresh; slippage on `minimum_output` |
| Bad split legs | Min input share, rate vs full-quote deviation, empty-pool exclusion |
| Soroban simulate failures | Clear API errors; UI guidance to refresh quote |
| Hybrid Classic+Soroban tx | Explicitly rejected at API; documented limitation |
| Solo bandwidth | Keep T2 focused on integration-ready assets and reproducible tooling; Audit Bank parallelized in T3 |

## Demo links

- Live app: https://lumagg.xyz  
- API health: https://api.lumagg.xyz/api/v1/health  
- Technical appendix: [docs/scf-build.md](./scf-build.md)

## Video script (2–3 min)

1. Problem: fragmented Stellar DEX liquidity (15s)
2. Show lumagg.xyz — XLM → USDC, quote, route breakdown with rates (45s)
3. Execute small swap — wallet sign, Horizon success (45s)
4. Show `scripts/scf-demo.sh` / API for integrators (30s)
5. Architecture: worker → Redis → API → aggregator contract (30s)
6. Ask: $100k / 5 months / three tranches; Audit Bank for contract (15s)

---

## SCF form paste (plain text)

SCF fields may show everything as one paragraph — that is fine. Copy the blocks below as-is.

### Tranche completion dates (calendar)

If T1 starts ~June 2026: **T1 = 30/07/2026**, **T2 = 30/08/2026**, **T3 = 15/10/2026** (T3 is 1.5 months after T2 for Audit Bank + SDK buffer).

### Tranche 1

Milestone: Production hardening and integrator-facing API docs. Key deliverables: (1) Production SLOs for API and worker (uptime, pool publish freshness). (2) Public integration guide and OpenAPI. (3) Monitoring and alerting. (4) CI running quote regression tests (scripts/scf-quote-regression.sh). Done when: SLOs documented, integration guide published, and regression runs in CI.

### Tranche 2

Milestone: Ecosystem integration readiness and route coverage improvements. Key deliverables: (1) Improve discovery and refresh for non-trivial pools and edge cases (Comet/Sushi factory coverage). (2) Quote latency and pool coverage optimization; publish before/after metrics. (3) Integration-ready package: versioned API/SDK docs, reproducible harness, sample app. Done when: documented metrics, published repo/docs artifacts, and a third party can integrate via quote + build_tx using the harness.

### Tranche 3

Milestone: Mainnet launch maturity and security-readiness completion.

Key deliverables: (1) Final production reliability pass for API, worker, and contract execution flows. (2) Security-readiness completion and audit track progression via Soroban Audit Bank (intake submitted, readiness passed, critical/high findings remediated where applicable). (3) TypeScript SDK or embeddable swap widget for ecosystem adopters. (4) End-to-end public demo package (video + reproducible scripts + docs). (5) Operational validation report: published service metrics (uptime, pool coverage, quote latency), example mainnet transaction links, and CI/regression output — self-serve evidence; no partner LOI required.

How completion is measured: (1) Stable end-to-end swap flow (quote → build → sign → submit) demonstrated publicly on mainnet. (2) Security-readiness artifacts completed and Audit Bank intake submitted. (3) Public final report with achieved KPIs and published integration-ready artifacts (SDK/widget, docs, harness).

### Team

Solo full-time founder. Scope: Soroban contract; Rust backend (router-engine, api-server, market-data-worker); Next.js frontend; mainnet ops (RPC, Redis, monitoring). Skills: Rust, Soroban/Stellar, API design, TypeScript/React, production reliability. GitHub: https://github.com/ligulfzhou/stellar-dex-agg · Live: https://lumagg.xyz · https://api.lumagg.xyz
