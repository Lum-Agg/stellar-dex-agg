LumAgg Tranche 1 — Deliverable 2 external integrator smoke

API=https://api.lumagg.xyz
USER_G=GDXRRY4HHIERMJBY62B4YJ25V3YNTMEOG3CQRLRHJ3P57Q57CYSJLPI2
Role=external friend / non-founder G-address (docs-only path)
Generated=2026-07-27T10:24Z
Command=USER_G=G... ./scripts/integrator-smoke.sh

Files:
- quote.json       GET /api/v1/quote response (is_split=true, Soroswap + Aquarius CLMM)
- build.json       POST /api/v1/build_tx request body (user_public_key = USER_G above)
- build_resp.json  POST /api/v1/build_tx response (unsigned_tx_xdr present, success=true)

Notes:
- Evidence is quote + unsigned XDR build only (no on-chain submit required for D2).
- Founder self-smoke must not be used alone for the “≥1 external developer” criterion.
