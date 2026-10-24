# Frontend Browser Regression

## Run locally

From `packages/frontend`:

```bash
npm ci
npx playwright install --with-deps chromium
npm test
npm run test:e2e
```

The Playwright command builds a static Next.js export and tests it at
`http://127.0.0.1:3105` in desktop and mobile Chromium layouts. Port 3105 must be
free. The test server binds only to localhost and never proxies API calls.

To use an already installed Chrome in an isolated test profile:

```bash
PLAYWRIGHT_CHANNEL=chrome npm run test:e2e
```

No real extension, wallet connection, secret key, funded account, API server, or
blockchain node is needed. The fixture simulates Freighter's public message
bridge and intercepts all external browser HTTP/WebSocket requests. Unexpected
API/RPC calls fail the test; no transaction is sent to a real network. The mock
wallet echoes a fixture XDR, not a cryptographic signature. These tests do not
replace real-wallet signing checks, contract tests, or small live-network
operational validation.

## Coverage

- Shared token URL restoration, including asynchronously loaded catalog tokens,
  invalid-token notices, reverse-pair URL updates, and reloads.
- External balance refresh on focus without requesting wallet access again.
- Anonymous visits do not initialize the wallet until an explicit connection.
- Wallet rejection never submits a transaction and leaves the form usable.
- Quote-to-build step mapping, wallet interaction, submission, retrying a
  transient status lookup error, and post-confirmation balance refresh.
- Confirmation through official RPC reads status without decoding unrelated,
  potentially unsupported XDR fields.
- Confirmation timeout is pending, not failed. Pending hash and submission
  channel survive reload; checking status never signs or submits again.
- Confirmed on-chain failure is terminal and keeps an explorer link.
- Explicitly stopping tracking requires acknowledgement that this does not
  cancel the transaction or prove it was never included.
- Light/dark preference persists and the layout fits desktop/mobile widths.

All API and wallet results are deterministic fixtures. The tests verify the
actual exported frontend, wallet SDK integration, and UI state transitions,
not production liquidity, quotes, balances, or transaction validity.

## Pending swap behavior

After accepted submission, only the public hash, account, network, and selected
submission channel are stored in `sessionStorage`. No signed XDR or secret is
stored. Tracking survives navigation/reload in that tab, but is not a global
cross-tab or cross-device submission lock. The swap form blocks a new submission
until the tracked result is checked or the user explicitly stops tracking.

Status checks use the saved channel even when the user changes the submission
toggle later. `NOT_FOUND`, timeout, and status-service errors do not mean chain
failure. Recovery is an explicit, one-shot **Check transaction status** action;
the page does not silently broadcast again or poll indefinitely. The explorer
link remains available when historical RPC data is no longer retained.

Closing the tab or clearing session storage can remove the tracking record;
verify an outstanding transaction on the explorer before initiating another.
Stopping tracking does not cancel the transaction.

## Artifacts and CI

The E2E build uses the reserved `https://lumagg-api.test` origin and writes
`out-e2e`, separate from the normal deployable `out` directory. **Never deploy
`out-e2e`.** Production builds continue to use `npm run build` without the E2E
environment flag. No test-only wallet bypass is added to application code.

CI installs Chromium and runs both viewport projects. Failure screenshots and
traces under `packages/frontend/test-results` are uploaded for seven days.
Locally, inspect a failure trace with:

```bash
npx playwright show-trace test-results/<failed-test>/trace.zip
```

Generated exports, reports, and traces are git-ignored. Unit tests continue to
run through Vitest; browser tests use `*.e2e.ts` so they are not collected by
Vitest.
