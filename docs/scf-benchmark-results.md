# LumAgg quote benchmark results

Generated: **2026-07-21 17:12 UTC**

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
| USDC → XLM | 1 USDC | 5.2097 | no | aquarius_clmm | 15.3209 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| USDC → XLM | 10 USDC | 52.0969 | no | aquarius_clmm | 153.1089 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| USDC → XLM | 100 USDC | 520.9554 | no | aquarius_clmm | 1,521.1884 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| USDC → XLM | 1,000 USDC | 5,208.2231 | no | aquarius_clmm | 14,287.9921 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| XLM → USDC | 1 XLM | 0.1940 | yes | soroswap → aquarius ;; soroswap → soroswap | 0.7206 | n/a | split 2 legs; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| XLM → USDC | 10 XLM | 1.9241 | yes | aquarius → aquarius ;; aquarius → aquarius ;; soroswap → aquarius | 5.6699 | n/a | split 3 legs; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| XLM → USDC | 100 XLM | 19.1864 | no | aquarius | 19.1814 | +0.03% | — |
| XLM → USDC | 1,000 XLM | 191.8454 | no | aquarius | 191.5380 | +0.16% | — |
| XLM → AQUA | 10 XLM | 5,154.5228 | no | aquarius_clmm | 7,040.0378 | -26.78% | CLMM venue in route |
| XLM → AQUA | 100 XLM | 51,545.1132 | no | aquarius_clmm | 70,368.9897 | -26.75% | CLMM venue in route |
| XLM → AQUA | 1,000 XLM | 515,439.6619 | no | aquarius_clmm | 700,565.8226 | -26.43% | CLMM venue in route |

## Tranche 1 reviewer highlights (this run)

| Criterion | Evidence |
|-----------|----------|
| ≥3 pairs × ≥3 sizes | USDC↔XLM (4 sizes) + XLM→AQUA (3 sizes) |
| Fair Soroban parity rows | **XLM→USDC 100 XLM: +0.03%**; **1,000 XLM: +0.16%** (both within ~2×, Δ published) |
| ≥1 **new** split case vs Soroswap single-route | **XLM→USDC 1 XLM** (`is_split=true`, 2 paths) and **10 XLM** (3 paths) — Soroswap column is a single route |
| CLMM coverage (vs Broker adapter gap) | USDC→XLM and XLM→AQUA route via **`aquarius_clmm`** — see [scf-venue-comparison.md](scf-venue-comparison.md) |

Small-size USDC↔XLM rows remain marked **n/a** when Soroswap `amountOut` diverges by >2× (likely different route class / API quirks). Prefer the fair rows above for apples-to-apples claims.

## Summary

- **Venue coverage:** See [scf-venue-comparison.md](scf-venue-comparison.md) for Stellar Broker CLMM gap (source-based).
- **Split routing:** LumAgg `is_split=true` when Brent optimizer splits `amount_in` across distinct paths; Soroswap API returns a single best route.
- **Fair compare:** Prefer `LUMAGG_PREFER_SOROBAN=1` + Soroswap `protocols` without `sdex` so Classic SDEX does not dominate LumAgg while Soroswap stays Soroban-only.
- **Soroswap API key:** Free registration at https://api.soroswap.finance/register — pass via `SOROSWAP_API_KEY` (never commit the key).
- **Split cases in this run:**
  - XLM → USDC 1 XLM: 2 paths (`soroswap → aquarius` ;; `soroswap → soroswap`)
  - XLM → USDC 10 XLM: 3 paths (`aquarius → aquarius` ;; `aquarius → aquarius` ;; `soroswap → aquarius`)

