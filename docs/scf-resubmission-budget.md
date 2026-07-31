# SCF #44 resubmission — budget & deliverables ($90k)

**Paste each “TRANCHE COPY BLOCK” section into the SCF dashboard rich-text field.**

---

## Budget summary

| | Amount | Completion date | SCF payout (cumulative) |
|---|--------|-----------------|-------------------------|
| **Total** | **$90,000** | Award end **Dec 31, 2026** | 100% |
| On award (pass) | — | ~Jul 2026 | **10% = $9,000** |
| Tranche 1 | **$26,000** | **Jul 31, 2026** | 30% cumulative |
| Tranche 2 | **$36,000** | **Aug 31, 2026** | 60% cumulative |
| Tranche 3 | **$28,000** | **Oct 15, 2026** | 100% cumulative |

### Shipped MVP — $0 from this grant (pre-award)

Live today: [lumagg.xyz](https://lumagg.xyz), [api.lumagg.xyz](https://api.lumagg.xyz), `market-data-worker` + Redis (6 Soroban venue types), `router-engine` + Brent split, mainnet aggregator (`split_swap`), swap UI shell, Telegram alerts, Comet/CLMM hydration.

### Post-submission & resubmission scope — not in original $100k application

Developed **after** initial SCF #44 submission (~Jun 2026): `aggregator.round_trip_swap` bot path, `contracts/vault` (`execute_round_trip`), differentiation docs + benchmark scripts.

**Added at resubmission (net-new deliverables):** on-chain **analytics indexer + dashboard**, **atomic arb operator stack** (mainnet validation), **OpenAPI / integrator API**, swap UI completion (logos, balance %), **third-party audit of aggregator + vault**.

Evidence: [scf-venue-comparison.md](scf-venue-comparison.md) · [scf-benchmark-results.md](scf-benchmark-results.md)

### Why $100k → $90k (only −$10k)

| Removed from grant ($0) — already shipped | Added to grant — net-new |
|-------------------------------------------|--------------------------|
| Event-driven pool state pipeline | Analytics indexer + dashboard |
| Router engine + quote API rebuild | Atomic arb + vault operator stack (post-submission code) |
| Aggregator mainnet production | External audit (**aggregator + vault**) |
| Swap frontend MVP | OpenAPI, API keys, `prefer_soroban` |
| Comet / extended DEX hydration | Token metadata, logos, balance UX |
| Internal “hardening” of worker/API | npm SDK publish + ≥2 integration validation paths |

We reduced **$10k** (not 25–40%) because the **original line items for shipped MVP are now $0 pre-award**, while **new deliverables** (analytics, post-submission arb+vault, dual-contract audit, integrator SDK/pilots) were added. No large “removed vs added” dollar reallocation — only a clearer scope split and a modest total trim.

---

# TRANCHE COPY BLOCK — 1

**Tranche #1 budget:** $26,000  
**Tranche #1 completion date:** 31/07/2026

---

**Note for reviewers:** Tranche 1 does **not** rebuild the shipped worker/router/API. It funds public **differentiation evidence**, **integrator API readiness**, **swap UI completion**, and **analytics indexer v0** (early data collection).

---

**[DELIVERABLE 1: LIVE BENCHMARK EXPANSION & DIFFERENTIATION MAINTENANCE]**

**Budget:** $1,500 (Grant-period expansion only — initial comparison docs exist pre-award as $0 evidence; this funds matrix growth + refresh through Jul 2026)

**Brief description:** Expand and maintain verifiable differentiation vs Soroswap (live quote benchmark), Stellar Broker (public router-contract adapter matrix — no Aquarius CLMM / Sushi CLMM), and wallets (xBull). **Net-new in Tranche 1:** grow benchmark coverage, refresh results as APIs change, and link the pack from the integrator guide (Deliverable 2). Repo entry points: `scripts/scf-benchmark.sh`, `docs/scf-venue-comparison.md`, `docs/scf-benchmark-results.md`.

**How to measure completion:**
- Benchmark matrix expanded to **≥3 pairs × ≥3 sizes** with dated rows in `docs/scf-benchmark-results.md` (fair rows documented: e.g. XLM→USDC 100–1,000 XLM; 10 XLM multi-leg split; XLM→AQUA via Aquarius CLMM)
- Stellar Broker CLMM gap cited from public GitHub (`stellar-broker/router-contract` `src/types/protocol.rs`)
- Reviewer can re-run `./scripts/scf-benchmark.sh` locally; README one-command instructions in repo
- **≥1 new** documented split case (`is_split=true`) vs Soroswap single-route output (not in resubmission snapshot)

**Estimated date of completion:** July 20, 2026

---

**[DELIVERABLE 2: INTEGRATOR-READY API — KEYS, OPENAPI & ROUTING CONTROLS]**

**Budget:** $9,000 (Engineering: partner API keys, OpenAPI spec, routing query params — **not** a router rewrite; includes link to Deliverable 1 benchmark pack)

**Brief description:** Make the **existing** public API adoptable by wallets and dApps: partner API key flow, documented rate limits, **OpenAPI** spec for `/quote`, `/build_tx`, `/tokens`, `/health`, integrator quickstart, and optional quote param **`prefer_soroban`** (exclude Classic DEX when integrators need Soroban-only comparison).

**How to measure completion:**
- Published integrator guide + OpenAPI (Swagger) in repo, linked from docs site
- Documented API key issuance process
- `prefer_soroban` (or equivalent) live on `/quote` with documented behavior
- ≥1 external developer completes quote + unsigned XDR build using docs only (feedback or PR)

**Estimated date of completion:** July 31, 2026

---

**[DELIVERABLE 3: SWAP UI — TOKEN METADATA, LOGOS, BALANCE & QUICK AMOUNTS]**

**Budget:** $6,500 (Engineering: server-side token metadata enrichment + frontend UX)

**Brief description:** Complete retail swap UX gaps on [lumagg.xyz](https://lumagg.xyz): enrich `/api/v1/tokens` with **logo URLs** (metadata pipeline + fallbacks), show wallet **spendable balance** for input token, quick amount chips **25% / 50% / 75% / 100%** (respect XLM reserve), tx **explorer link** on success, and basic slippage control.

**How to measure completion:**
- ≥50 routed tokens return logo or deterministic fallback via API
- Connected wallet shows spendable `token_in` balance
- Four percentage chips populate `amount_in`; disabled when disconnected
- Mobile-responsive; quote → build_tx → sign flow unchanged
- No regression to production API

**Estimated date of completion:** July 31, 2026

---

**[DELIVERABLE 4: ON-CHAIN ANALYTICS — INDEXER V0]**

**Budget:** $9,000 (Engineering: tx/event ingestion — dashboard UI in Tranche 3)

**Brief description:** Start production analytics (**not implemented today**). Indexer ingests LumAgg **aggregator contract** invocations from Horizon/Soroban RPC: swap volume, tx count, function breakdown (**`split_swap` vs `round_trip_swap`**), unique wallet addresses, per-leg **DEX attribution** (Soroswap, Aquarius, Phoenix, Sushi, Comet). Store time-series for ≥30 days by Tranche 3.

**How to measure completion:**
- Indexer running against mainnet aggregator contract ID(s)
- Internal or API export: daily volume, tx count, unique users, per-function counts
- Volume attribution spec drafted in `docs/`
- Data pipeline documented for Tranche 3 dashboard

**Estimated date of completion:** July 31, 2026

---

# TRANCHE COPY BLOCK — 2

**Tranche #2 budget:** $36,000  
**Tranche #2 completion date:** 31/08/2026

---

**Note for reviewers:** Tranche 2 funds **npm SDK release**, **first production-ready atomic arbitrage operator stack** (code written **post-submission, Jun 2026** — not in original application), and **integrator integration validation across ≥2 paths**. Does **not** fund rebuilding swap frontend or pre-submission router/worker.

---

**[DELIVERABLE 5: NPM TYPESCRIPT SDK & INTEGRATION EXAMPLES]**

**Budget:** $11,000 (Engineering: npm publish, types, CI, examples)

**Brief description:** Publish `packages/sdk` to npm (semver, typed `quote()` / `buildTx()`, sub-route leg parsing). Ship CLI example + minimal Freighter dApp snippet. Optional thin Rust client stub in `crates/sdk` for parity.

**How to measure completion:**
- Public npm package (Apache-2.0)
- ≥2 working examples in repo
- Documented third-party swap-in-under-30-min test
- API reference aligned with `/api/v1/quote` and `/api/v1/build_tx`

**Estimated date of completion:** August 20, 2026

---

**[DELIVERABLE 6: ATOMIC ARBITRAGE OPERATOR STACK — POST-SUBMISSION CODE → MAINNET]**

**Budget:** $17,000 (Engineering: vault mainnet deploy, arb bot validation, operator docs, testnet→mainnet path, RPC/mainnet testing budget)

**Brief description:** **Net-new since initial application (Jun 2026):** `crates/arbitrage` scanner + `contracts/vault` (`execute_round_trip`) + `aggregator.round_trip_swap` integration. Grant completes **operator-grade release** (not greenfield router work):
- Deploy **arb-only vault** on mainnet (`initialize`, `deposit`, `add_caller`)
- Harden scanner: `ARB_VAULT_CONTRACT`, `DRY_RUN` → simulated → controlled live submit
- Operator playbook: pooled capital, multi-caller gas-only bots, safety limits
- Ecosystem effect: atomic arb increases cross-venue DEX pool turnover and price alignment

**How to measure completion:**
- Mainnet vault contract ID + runbook (`contracts/vault/README.md`)
- Published operator doc in `docs/` (self-deploy: aggregator + vault + bot)
- ≥10 successful DRY_RUN or RPC-simulated prepared txs with route labels
- ≥1 on-chain round-trip (`round_trip_swap` or `vault.execute_round_trip`) on mainnet or public testnet with documented tx hash(s)
- Risk/limitations section documented (arb-only vault, not retail yield)

**Estimated date of completion:** August 31, 2026

---

**[DELIVERABLE 7: INTEGRATOR INTEGRATION VALIDATION (≥2 PATHS)]**

**Budget:** $8,000 (Engineering support + documentation)

**Brief description:** Validate **≥2 integrator adoption paths** for the open-source stack. Integrators may use the self-hosted API or the public API with the npm SDK; hosted API use is not required.

- **Path A:** In-repo reference client (LumAgg swap UI or SDK demo app) completes `quote → build_tx` end to end.
- **Path B:** External developer, reviewer, or community integrator completes the same flow using the published docs; feedback or anonymized confirmation is documented.

The output is reproducible integration evidence, not guaranteed SaaS onboarding.

**How to measure completion:**
- Two paths documented with reproducible steps; Path B may use an anonymized role
- Each path completes `quote → build_tx` through the SDK or REST API
- Self-host quickstart and an under-30-minute walkthrough documented
- Path B feedback incorporated into the SDK or integrator guide

**Estimated date of completion:** August 31, 2026

---

# TRANCHE COPY BLOCK — 3

**Tranche #3 budget:** $28,000  
**Tranche #3 completion date:** 15/10/2026

---

**Note for reviewers:** Tranche 3 funds **public analytics dashboard**, **third-party smart contract audit** (aggregator + vault), and **grant close-out**. Audit follows vault deploy and arb stack freeze in Tranche 2.

---

**[DELIVERABLE 8: ANALYTICS DASHBOARD & VOLUME REPORT]**

**Budget:** $8,000 (Engineering: dashboard and/or public `/stats` API on Tranche 1 indexer)

**Brief description:** Ship user-facing and reviewer-facing analytics built on Deliverable 4 indexer:
- Total swap volume (notional) and tx count
- **`split_swap` vs `round_trip_swap`** (and `vault.execute_round_trip` when live)
- Unique trader addresses
- **Per-DEX / per-pool** leg attribution
- Arb impact section: sample round-trip txs → pools touched
- Optional: worker pool-coverage health (% CLMM pools with complete tick coverage)

**How to measure completion:**
- Public stats page or documented `/stats` API + CSV export
- ≥30 days of indexed data (or since indexer start)
- Volume attribution spec finalized in `docs/`
- Sample grant report table: user swaps + arb txs → DEX volume

**Estimated date of completion:** October 5, 2026

---

**[DELIVERABLE 9: THIRD-PARTY SMART CONTRACT AUDIT — AGGREGATOR + VAULT]**

**Budget:** $16,000 (External audit firm or Stellar audit bank; remediation engineering)

**Brief description:** Professional security audit of mainnet **aggregator** (`split_swap`, `round_trip_swap`, DEX CPI auth) and **arb vault** (`execute_round_trip`, caller allowlist, fund flow). Remediate critical/high findings; upgrade WASM if required.

**How to measure completion:**
- Signed audit scope + final report (or audit bank engagement letter + report)
- All critical/high issues resolved or accepted with documented rationale
- Post-remediation contract IDs / upgrade tx hashes recorded

**Estimated date of completion:** October 1, 2026

---

**[DELIVERABLE 10: GRANT CLOSE-OUT — DEMO, SELF-HOST KIT & FINAL REPORT]**

**Budget:** $4,000 (Documentation, demo video, handoff)

**Brief description:** Final SCF report; 5-minute demo (swap UI with logos/balance %, split route, analytics snapshot, arb/vault architecture); **docker-compose self-host kit** for integrators; **Protocol 27** compatibility test checklist; 6-month maintenance plan.

**How to measure completion:**
- Final report markdown/PDF in repo with all deliverable links
- Public demo video (quote → split → on-chain swap; arb architecture segment)
- `docker-compose` or equivalent documented self-host path (API + worker + Redis)
- P27 testnet regression notes after network upgrade
- Maintenance plan: RPC, monitoring, upgrade path

**Estimated date of completion:** October 15, 2026

---

## Future work (unfunded — not in this $90k scope)

- WebSocket streaming quotes (session-based integrators)
- Integrator referral / fee-share model
- User swap history UI
- Limit orders / TWAP
- Multi-region API deployment

---

## Resubmission feedback — paste into dashboard

```markdown
## Resubmission changelog (SCF #44 — LumAgg)

Thank you for the panel feedback. Below is our change log mapped to your **three** requested edits.

---

### 1. Differentiation (vs Soroswap, xBull, Stellar Broker)

**Positioning:** Open-source, self-hostable integrator/operator infra — not Stellar Broker (hosted session router) and not xBull (wallet).

**Stellar Broker — no live API benchmark (deadline constraint):**
We did **not** run live Stellar Broker API quotes before resubmission. Hosted Broker access requires an **application/approval**; we did not receive credentials in time for SCF #44 revision. Broker comparison below is from the **public open-source router contract** only (`broker/router-contract`, same as github.com/stellar-broker/router-contract) — adapter coverage and on-chain fee model — **not** a head-to-head execution benchmark vs Broker’s hosted service.

**Venue coverage (public router-contract, Jun 2026):**
- LumAgg routes **6 Soroban families** (Soroswap, Aquarius xy=k + stable + **CLMM**, Phoenix, **Sushi V3 CLMM**, Comet weighted) **plus Classic SDEX** when it wins on price.
- Broker **public router-contract** adapters: AquaConstant, AquaStable, Soroswap, Comet, Phoenix only — **no Aquarius CLMM, no Sushi V3 CLMM** in `src/adapters/`.
- xBull: wallet product; not a multi-venue aggregator.

**Broker on-chain fee model (router contract `swap()`):**
The Broker router contract **charges swap fees on-chain** — not a zero-fee pass-through router. Each `swap()` call takes `vfee` and `ffee` (rates in **‰**, per thousand):
- **vfee:** variable fee on “profit” (actual output above estimated/minimum)
- **ffee:** fixed fee on total bought amount
Fees are deducted from the trader’s output, accumulated in a configured `fee_token` on the contract, and **admin `withdraw()`** pulls them. Example from contract tests: vfee=150‰ + ffee=10‰ on a swap charges measurable USDC fees to the contract balance. **LumAgg’s open aggregator contract does not implement this Broker-style vfee/ffee skim** — integrators/wallets keep full quoted output subject to slippage only.

**Live quote benchmark vs Soroswap only** (LumAgg `api.lumagg.xyz` vs Soroswap API, run **2026-06-25 10:33 UTC**; reproducible via `./scripts/scf-benchmark.sh`):

| Pair | Size | LumAgg | Split? | Primary route | vs Soroswap |
|------|------|--------|--------|---------------|-------------|
| XLM → USDC | 1,000 XLM | 184.15 USDC | no | Classic DEX | **+0.02%** (parity at size) |
| XLM → USDC | 1 XLM | 0.19 USDC | **yes (2 legs)** | Aquarius + Soroswap split | Soroswap single-route; LumAgg splits across venues |
| XLM → USDC | 100 XLM | 18.42 USDC | no | **Aquarius CLMM** | -2.92% (fair Soroban row; CLMM route) |
| XLM → AQUA | 100 XLM | 51,364 AQUA | no | **Aquarius CLMM only** | n/a (>3× unit gap — CLMM venue Soroswap/Broker open router lacks) |

**Execution model vs Soroswap:** Soroswap API returns a single best route; LumAgg runs Brent **multi-path split** (`is_split=true`) and atomic on-chain `split_swap` / `round_trip_swap`.

**Post-submission operator stack (Jun 2026):** `round_trip_swap` + arb vault (`execute_round_trip`) + `crates/arbitrage` — self-deploy atomic arb; not offered as open-source operator infra by Broker or Soroswap public repos.

---

### 2. Timeline

Our milestones were always Q3–Q4 **2026**, not 2027. We aligned all deliverable dates and award end to **Dec 31, 2026**:

| Tranche | Completion |
|---------|------------|
| 1 | Jul 31, 2026 |
| 2 | Aug 31, 2026 |
| 3 | Oct 15, 2026 |

Grant-funded deliverables complete by **Oct 15, 2026**; remaining time is buffer for reporting only.

---

### 3. Net-new scope vs already-shipped MVP

**Already live at resubmission ($0 from grant — not re-funded):** pool-state worker, public quote API (api.lumagg.xyz), router-engine + Brent split, mainnet aggregator (`split_swap`), swap UI shell (lumagg.xyz), monitoring/alerts, Comet/CLMM hydration. These were the bulk of our **original $100k line items** and are linked live in this submission.

**Added after initial application (Jun 2026) — not in original $100k scope:**
- Atomic arb operator stack: `aggregator.round_trip_swap`, `contracts/vault` (`execute_round_trip`), `crates/arbitrage`
- On-chain analytics (indexer v0 + public dashboard)
- Dual-contract external audit (aggregator + vault)
- OpenAPI / integrator API + API keys; npm SDK; ≥2 integration validation paths
- Swap UI completion (token logos, wallet balance %, quick amounts)

**Why total is only $10k lower ($100k → $90k):** We reclassified shipped MVP as **$0 pre-award** (addressing “do not fund hardening existing components”), **and** added the net-new ecosystem work above that did not exist when we first applied. A larger cut would underfund analytics, arb/vault mainnet, and the two-contract audit; **$90k** reflects panel feedback while still funding genuinely new deliverables.

**Tranche alignment:** T1 = integrator API + UI gaps + indexer v0; T2 = npm SDK + arb/vault mainnet + pilots; T3 = analytics dashboard + audit + close-out.

**Links:** https://lumagg.xyz · https://api.lumagg.xyz · https://github.com/Lum-Agg/stellar-dex-agg
```
