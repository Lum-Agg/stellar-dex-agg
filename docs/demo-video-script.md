# Demo video script (~5 min) — Tranche 3 Deliverable 10

Record screen + voiceover. Target audience: SCF reviewers and integrators.

## 0:00 — Intro (30s)

- **Title card:** LumAgg — Stellar DEX aggregator + atomic arb vault
- **Say:** Multi-venue Soroban routing, split swaps, public API, on-chain analytics, and mainnet arb stack.
- **Show:** https://lumagg.xyz

## 0:30 — Swap UI (60s)

- Open swap page; select XLM → USDC
- Point out: token logos, wallet balance, % quick amounts
- Run quote; highlight **split route** if shown (two legs / percentages)
- Optional: connect Freighter, sign small swap
- **Explorer link** after submit (or show recent tx from stats)

## 1:30 — Integrator API (90s)

- Terminal: `./scripts/integrator-smoke.sh` or `npx tsx packages/sdk/examples/quote-build.ts`
- Show quote JSON: `expected_output`, `is_split`, `sub_routes`
- Show `build_tx` → `unsigned_tx_xdr` prefix
- Mention: `prefer_soroban=1`, partner API keys, OpenAPI at lumagg.xyz/docs

## 3:00 — Analytics (45s)

- Open https://lumagg.xyz/stats
- Show daily tx count, DEX breakdown
- `curl https://api.lumagg.xyz/api/v1/stats?format=csv` (grant export)

## 3:45 — Arb architecture (60s)

- Diagram or slide: **Vault** → **Aggregator** `round_trip_swap` → profit to vault
- Show [arb-operator.md](./arb-operator.md) or stellar.expert tx from [arb-evidence-snapshot.md](./arb-evidence-snapshot.md)
- Mention: caller allowlist, fee gate, Telegram hourly report

## 4:45 — Close (15s)

- Self-host: `docker-compose.selfhost.yml`
- npm SDK, maintenance plan, P27 checklist
- **End card:** GitHub repo + api.lumagg.xyz

## Production tips

- 1080p, 16:9; hide desktop clutter
- Pre-warm quote (run once before record)
- Upload unlisted YouTube / Loom; link in [scf-final-report.md](./scf-final-report.md)
