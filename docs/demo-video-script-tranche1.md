# Demo video script (~5 min) — SCF Tranche 1 completion form

Upload as **unlisted** YouTube / Loom / Drive. Paste the URL into **Deliverable Verification - Video**.

Audience: SCF reviewers verifying D1–D4. Do **not** spend time on arb vault / npm SDK (those are T2/T3).

---

## Pre-flight (do once before record)

1. Browser: clean window, 1920×1080, hide bookmarks bar; English UI if possible.
2. Tabs ready (left → right):
   - https://lumagg.xyz
   - https://lumagg.xyz/docs
   - https://lumagg.xyz/stats
   - https://github.com/Lum-Agg/stellar-dex-agg (or your public repo)
   - Terminal in repo root
3. Terminal pre-run (so live take is fast):
   ```bash
   # D1 — optional, show results file instead of full re-run if slow
   head -40 docs/scf-benchmark-results.md

   # D2 — use friend G or any funded mainnet G (do not need to submit tx)
   USER_G=GYourFriendAddress ./scripts/integrator-smoke.sh
   # Keep OUT dir open, or open docs/evidence/d2-integrator-smoke/
   ```
4. Freighter: connected on mainnet (optional for UI segment). If you skip signing, still show logos / balance / % chips.
5. Pre-warm one quote on the swap page (XLM → USDC) so the live take is snappy.

---

## Title card (optional, 5s)

Text on screen:

> LumAgg — SCF #44 Tranche 1  
> Integrator API · Swap UX · Analytics indexer · Differentiation evidence  
> https://lumagg.xyz · https://api.lumagg.xyz

---

## 0:00–0:25 — Intro

**Show:** lumagg.xyz homepage / swap.

**Say:**
> This is LumAgg, a Stellar DEX aggregator. Tranche 1 delivers four things: live differentiation benchmarks, an integrator-ready public API, completed swap UX, and an on-chain analytics indexer. I’ll walk through each with live evidence.

---

## 0:25–1:25 — D3 Swap UI (~60s)

**Show:** https://lumagg.xyz — select **XLM → USDC**.

**Do / point:**
1. Token logos visible in the picker / pair row.
2. Connect wallet (or already connected) → **spendable balance** on input token.
3. Click **25% / 50% / 75% / 100%** chips; note XLM keeps reserve.
4. Hit quote → if split, highlight **two legs / percentages / DEX names**.
5. Optional: sign a tiny swap and open the **explorer link**. If not signing: say “same quote → build_tx → Freighter path; I’ll skip broadcast to save time.”

**Say:**
> Deliverable 3 closes retail UX gaps: logos from the tokens API, wallet balance, quick amounts, and explorer link after submit. Routing still goes through the public quote and build_tx APIs.

---

## 1:25–2:55 — D2 Integrator API (~90s)

**Show A — Docs (20s):** https://lumagg.xyz/docs  
Point to OpenAPI / integrator guide, mention `prefer_soroban=1` and API keys.

**Show B — Terminal smoke (70s):**

```bash
# If re-running live:
USER_G=GDXRRY4HHIERMJBY62B4YJ25V3YNTMEOG3CQRLRHJ3P57Q57CYSJLPI2 \
  ./scripts/integrator-smoke.sh

# Or open committed evidence:
ls docs/evidence/d2-integrator-smoke/
jq '.success, .data.is_split, .data.sub_routes | length' docs/evidence/d2-integrator-smoke/quote.json
jq '.success, .data.unsigned_tx_xdr[:60]' docs/evidence/d2-integrator-smoke/build_resp.json
```

**Point on screen:**
- `success: true`
- `is_split: true` + `sub_routes`
- `unsigned_tx_xdr` prefix (do **not** scroll forever)
- README: external / non-founder `USER_G`

**Say:**
> Deliverable 2: partners can quote and get an unsigned XDR from docs alone. Here is an external G-address smoke — quote plus build_tx — no founder key. OpenAPI and the integrator guide are linked from the docs site; prefer_soroban excludes Classic when integrators need Soroban-only comparison.

---

## 2:55–3:50 — D1 Differentiation (~55s)

**Show:**
1. `docs/scf-venue-comparison.md` (GitHub or local) — Broker adapter gap / LumAgg venue list.
2. `docs/scf-benchmark-results.md` — dated table rows (XLM→USDC sizes, split row, prefer_soroban fair row).
3. Optional one-liner: `./scripts/scf-benchmark.sh` exists for reviewers to re-run.

**Say:**
> Deliverable 1 is verifiable differentiation, not marketing slides. We maintain a live Soroswap quote benchmark and a public Broker router-contract comparison — including CLMM coverage Broker’s open router lacks. Reviewers can re-run the benchmark script; results are dated in the repo.

---

## 3:50–4:45 — D4 Analytics indexer (~55s)

**Show:** https://lumagg.xyz/stats

**Point:**
- Daily / recent tx count and volume
- `split_swap` vs `round_trip_swap` if shown
- Per-DEX breakdown

**Then terminal (optional 10s):**
```bash
curl -sS 'https://api.lumagg.xyz/api/v1/stats?format=csv' | head -5
```

**Say:**
> Deliverable 4 is the production indexer v0: mainnet aggregator invocations, daily volume and tx counts, function breakdown, and per-DEX leg attribution. Dashboard UI polish continues in a later tranche; the data pipeline and /stats export are live now.

---

## 4:45–5:00 — Close (15s)

**Show:** GitHub repo root or end card.

**Say:**
> That’s Tranche 1: benchmarks, integrator API with external smoke evidence, completed swap UX, and live analytics. Links are in the completion form — lumagg.xyz, api.lumagg.xyz, and the repo evidence folder. Thanks for reviewing.

**End card text:**
> https://lumagg.xyz  
> https://api.lumagg.xyz  
> https://github.com/Lum-Agg/stellar-dex-agg  
> Evidence: docs/evidence/d2-integrator-smoke/

---

## Timing cheat-sheet

| Time | Segment | Deliverable |
|------|---------|-------------|
| 0:00 | Intro | — |
| 0:25 | Swap UI | D3 |
| 1:25 | Docs + smoke | D2 |
| 2:55 | Benchmark docs | D1 |
| 3:50 | /stats + CSV | D4 |
| 4:45 | Close | — |

If you run long: cut Freighter signing first, then cut live `scf-benchmark.sh`.

---

## Form paste (video description / Additional Verification)

```text
Tranche 1 demo (~5 min): D3 swap UX (logos, balance, % chips) → D2 integrator docs + external quote/build_tx smoke → D1 benchmark & venue comparison docs → D4 live /stats + CSV export.
Evidence folder: docs/evidence/d2-integrator-smoke/
```

---

## Recording tips

- Loom or unlisted YouTube; 1080p 16:9; mic close, quiet room.
- Cursor highlight / zoom on JSON keys (`is_split`, `unsigned_tx_xdr`).
- Don’t open private keys, `.env`, or Telegram bot tokens.
- One rehearsal take, then one real take — don’t over-edit.
