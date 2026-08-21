# LumAgg Freighter browser swap example

Minimal mainnet demo: **quote → build → Freighter sign → direct Soroban RPC submit → confirm**.

## Run

```bash
# from repo root — build SDK first
cd packages/sdk && npm run build

cd examples/browser-swap
npm install
npm run dev
```

Open the printed Vite URL (default http://localhost:5179), connect Freighter (Public network), then swap.

- **Dry-run** (default on): stops after Freighter signs — no chain submit.
- Uncheck dry-run to submit directly to `https://mainnet.sorobanrpc.com` and poll
  the transaction through the LumAgg API client. The optional LumAgg
  `POST /api/v1/submit_tx` proxy can be used instead when an integration cannot
  reach a Soroban RPC directly.

Optional: `VITE_API_URL=http://127.0.0.1:3100 npm run dev`

## Requirements

- Freighter extension on **Public** network
- Funded `G…` with XLM
- USDC trustline for buy asset (demo swaps XLM → USDC)
