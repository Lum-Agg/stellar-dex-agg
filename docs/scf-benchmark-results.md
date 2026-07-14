# LumAgg quote benchmark results

Generated: **2026-07-14 00:03 UTC**

- LumAgg API: `https://api.lumagg.xyz` (`prefer_soroban=1`)
- Soroswap API: `https://api.soroswap.finance` protocols=`soroswap,phoenix,aqua` (key provided)

Reproduce:

```bash
./scripts/scf-benchmark.sh
# Soroban-only fair compare:
LUMAGG_PREFER_SOROBAN=1 SOROSWAP_PROTOCOLS=soroswap,phoenix,aqua SOROSWAP_API_KEY=sk_... ./scripts/scf-benchmark.sh
# Full compare (include SDEX on both sides when LumAgg omits prefer_soroban):
SOROSWAP_PROTOCOLS=soroswap,phoenix,aqua,sdex SOROSWAP_API_KEY=sk_... ./scripts/scf-benchmark.sh
OUTPUT=docs/scf-benchmark-results.md ./scripts/scf-benchmark.sh
```

> **Interpretation:** Use `LUMAGG_PREFER_SOROBAN=1` + Soroswap without `sdex` for Soroban-only rows. Include `sdex` in `SOROSWAP_PROTOCOLS` when comparing full aggregation. Positive Δ = LumAgg higher output for same `amount_in`.

| Pair | Size | LumAgg out | Split | Sources | Soroswap out | Δ vs Soroswap | Notes |
|------|------|------------|-------|---------|--------------|---------------|-------|
| USDC → XLM | 1 USDC | 5.5343 | no | sushi | 39.8184 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| USDC → XLM | 10 USDC | 55.3429 | no | sushi | 397.5412 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| USDC → XLM | 100 USDC | 553.3874 | no | sushi | 3,912.2814 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| USDC → XLM | 1,000 USDC | 5,531.8914 | no | aquarius_clmm | 33,761.4350 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| XLM → USDC | 1 XLM | 0.1856 | yes | soroswap → soroswap ;; soroswap → soroswap ;; soroswap → soroswap | 0.6976 | n/a | split 3 legs; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| XLM → USDC | 10 XLM | 1.8039 | no | aquarius_clmm | 5.5136 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| XLM → USDC | 100 XLM | 18.0387 | no | aquarius_clmm | 18.0492 | -0.06% | CLMM venue in route |
| XLM → USDC | 1,000 XLM | 180.3834 | no | aquarius_clmm | 180.4751 | -0.05% | CLMM venue in route |
| XLM → AQUA | 10 XLM | 5,094.0778 | no | aquarius → aquarius | 14,645.6265 | n/a | ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| XLM → AQUA | 100 XLM | 50,913.5404 | no | aquarius_clmm | 146,350.5060 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| XLM → AQUA | 1,000 XLM | 509,107.7012 | no | aquarius_clmm | 1,453,011.7464 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |

## Summary

- **Venue coverage:** See [scf-venue-comparison.md](scf-venue-comparison.md) for Stellar Broker CLMM gap (source-based).
- **Split routing:** LumAgg `is_split=true` when Brent optimizer splits `amount_in` across distinct paths; Soroswap API returns a single best route.
- **Fair compare:** Prefer `LUMAGG_PREFER_SOROBAN=1` + Soroswap `protocols` without `sdex` so Classic SDEX does not dominate LumAgg while Soroswap stays Soroban-only.
- **Soroswap API key:** Free registration at https://api.soroswap.finance/register — pass via `SOROSWAP_API_KEY` (never commit the key).
- **Split cases in this run:**
  - XLM → USDC 1 XLM: 3 paths (`soroswap → soroswap` ;; `soroswap → soroswap` ;; `soroswap → soroswap`)

## What's new vs 2026-06-25 resubmission snapshot

| Item | 2026-06-25 | **2026-07-14 (this run)** |
|------|------------|---------------------------|
| LumAgg mode | all venues (Classic often won USDC↔XLM) | **`prefer_soroban=1`** (fair vs Soroswap Soroban protocols) |
| Fair rows (Δ) | XLM→USDC 100 / 1,000 XLM (−2.92% / +0.02%) | XLM→USDC 100 / 1,000 XLM (**−0.06% / −0.05%**, Aquarius CLMM) |
| New split case | XLM→USDC 1 XLM: 2 paths (Aquarius + Soroswap) | XLM→USDC 1 XLM: **3 paths** (Soroswap×3) — **new** vs prior split shape |
| USDC→XLM | Classic DEX | **Sushi / Aquarius CLMM** (no Classic under prefer_soroban) |
| XLM→AQUA | Aquarius CLMM; delta n/a | Aquarius xy=k / CLMM; Soroswap still >2× → **n/a** |

**SCF takeaway:** At 100–1,000 XLM→USDC, LumAgg tracks Soroswap within **0.1%** on Soroban-only quotes while additionally demonstrating **multi-path `is_split`** and **Sushi + Aquarius CLMM** venue coverage.

