# Integrator DX Implementation Plan

> **For agentic workers:** Execute task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Align OpenAPI + `@lumagg/sdk@0.2.0` with live wallet/tx APIs, ship a Freighter browser demo, and update integrator docs.

**Architecture:** No api-server changes. Document existing handlers in OpenAPI; extend `LumAggClient` with camelCase wrappers; Vite SPA signs via Freighter and submits via SDK.

**Tech Stack:** OpenAPI 3, TypeScript SDK (`tsc`), Vite + `@stellar/freighter-api`, markdown docs, hand-maintained `ApiReference.tsx`.

**Spec:** `docs/superpowers/specs/2026-07-22-integrator-dx-design.md`

---

## File map

| File | Role |
|------|------|
| `docs/openapi.yaml` | Add wallet/tx paths + schemas |
| `packages/sdk/src/index.ts` | New client methods + types |
| `packages/sdk/package.json` | Version `0.2.0` |
| `packages/sdk/README.md` | Document 0.2 APIs + browser-swap |
| `packages/sdk/examples/browser-swap/*` | Vite Freighter demo |
| `docs/integrator-guide.md` | Browser section + endpoint/SDK table |
| `docs/integrator-guide.zh-CN.md` | Mirror |
| `packages/frontend/src/components/docs/ApiReference.tsx` | Short endpoint entries |

---

### Task 1: OpenAPI wallet + tx paths

**Files:**
- Modify: `docs/openapi.yaml`

- [ ] **Step 1:** Bump `info.version` to `1.1.0`. Add tags `wallet`, `tx`.

- [ ] **Step 2:** Append paths matching handlers (snake_case wire fields):

```yaml
  /api/v1/balance:
    get:
      tags: [wallet]
      summary: Single SAC balance + trustline hint
      parameters:
        - name: account
          in: query
          required: true
          schema: { type: string }
        - name: token
          in: query
          required: true
          schema: { type: string }
      responses:
        '200':
          description: BalanceResponse
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/BalanceResponse'
  /api/v1/balances:
    get:
      tags: [wallet]
      summary: Common-token balances for account
      parameters:
        - name: account
          in: query
          required: true
          schema: { type: string }
      responses:
        '200':
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/BalancesResponse'
  /api/v1/account:
    get:
      tags: [wallet]
      summary: Account sequence via Soroban RPC
      parameters:
        - name: account
          in: query
          required: true
          schema: { type: string }
      responses:
        '200':
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/AccountResponse'
  /api/v1/classic_asset:
    get:
      tags: [wallet]
      summary: Resolve SAC contract to classic code/issuer
      parameters:
        - name: contract
          in: query
          required: true
          schema: { type: string }
      responses:
        '200':
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ClassicAssetResponse'
  /api/v1/ledger/latest:
    get:
      tags: [wallet]
      summary: Latest closed ledger sequence
      responses:
        '200':
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/LatestLedgerResponse'
  /api/v1/submit_tx:
    post:
      tags: [tx]
      summary: Submit signed XDR (fast enqueue)
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              required: [signed_tx_xdr]
              properties:
                signed_tx_xdr: { type: string }
      responses:
        '200':
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/SubmitTxResponse'
  /api/v1/tx_status:
    get:
      tags: [tx]
      summary: Poll inclusion after submit_tx
      parameters:
        - name: hash
          in: query
          required: true
          schema: { type: string }
      responses:
        '200':
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/TxStatusResponse'
```

Schemas (under `components/schemas`):

- `BalanceResponse`: `success`, `balance?`, `has_trustline?`, `error?`
- `BalancesResponse`: `success`, `account`, `scope`, `tokens_queried[]`, `balances` (map string→string), `has_trustline` (map string→bool), `updated_at_ms`, `error?`
- `AccountResponse`: `success`, `sequence?`, `error?`
- `ClassicAssetResponse`: `success`, `code?`, `issuer?`, `error?`
- `LatestLedgerResponse`: `success`, `sequence`, `error?`
- `SubmitTxResponse`: `success`, `hash?`, `status?`, `error?`
- `TxStatusResponse`: `success`, `hash?`, `status?`, `confirmed`, `error?`

- [ ] **Step 3:** Commit `docs: document wallet and tx endpoints in OpenAPI 1.1`

---

### Task 2: SDK wallet + tx methods

**Files:**
- Modify: `packages/sdk/src/index.ts`
- Modify: `packages/sdk/package.json` (version `0.2.0`)

- [ ] **Step 1:** Add types + methods to `LumAggClient` (after `getPriceHistory`, before `getStats`):

```typescript
export interface BalanceResult {
  balance?: string;
  hasTrustline?: boolean;
}

export interface BalancesResult {
  account: string;
  scope: string;
  tokensQueried: string[];
  balances: Record<string, string>;
  hasTrustline: Record<string, boolean>;
  updatedAtMs: number;
}

export interface AccountResult {
  sequence: string;
}

export interface ClassicAssetResult {
  code?: string;
  issuer?: string;
}

export interface SubmitTxResult {
  hash: string;
  status?: string;
}

export interface TxStatusResult {
  hash?: string;
  status?: string;
  confirmed: boolean;
  error?: string;
}

export interface WaitForTxOptions {
  timeoutMs?: number;
  intervalMs?: number;
}
```

Methods throw on `!json.success` / missing required fields. `waitForTx`:

```typescript
async waitForTx(hash: string, opts: WaitForTxOptions = {}): Promise<TxStatusResult> {
  const timeoutMs = opts.timeoutMs ?? 60_000;
  const intervalMs = opts.intervalMs ?? 1_000;
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const st = await this.getTxStatus({ hash });
    if (st.confirmed || st.status === 'FAILED') return st;
    await new Promise((r) => setTimeout(r, intervalMs));
  }
  throw new Error(`waitForTx timeout after ${timeoutMs}ms (hash=${hash})`);
}
```

- [ ] **Step 2:** `cd packages/sdk && npm run build` — expect clean `tsc`.

- [ ] **Step 3:** Smoke against prod (no Freighter):

```bash
node -e "
const { LumAggClient } = require('./dist/index.js');
(async () => {
  const c = new LumAggClient({ apiUrl: 'https://api.lumagg.xyz' });
  const b = await c.getBalance({
    account: 'GA6RKSBPI2TSP52OW2IJTPK7LRMX24DF42KF3FBGBNMBYCV6NPDMOCBY',
    token: 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA',
  });
  console.log('balance', b);
  const led = await c.getLatestLedger();
  console.log('ledger', led);
  const st = await c.getTxStatus({ hash: '0'.repeat(64) });
  console.log('tx_status', st);
})();
"
```

Expected: numeric/string balance or undefined; ledger sequence > 0; tx_status with `confirmed: false` / `NOT_FOUND`.

- [ ] **Step 4:** Commit `feat(sdk): add wallet and tx helpers for 0.2.0`

---

### Task 3: Freighter browser-swap demo

**Files:**
- Create: `packages/sdk/examples/browser-swap/package.json`
- Create: `packages/sdk/examples/browser-swap/vite.config.ts`
- Create: `packages/sdk/examples/browser-swap/tsconfig.json`
- Create: `packages/sdk/examples/browser-swap/index.html`
- Create: `packages/sdk/examples/browser-swap/src/main.ts`
- Create: `packages/sdk/examples/browser-swap/src/style.css`
- Create: `packages/sdk/examples/browser-swap/README.md`

- [ ] **Step 1:** Scaffold Vite app. Depend on `"@lumagg/sdk": "file:../.."` and `"@stellar/freighter-api"`.

- [ ] **Step 2:** Implement `main.ts` flow: connect → balance/trustline → quoteAndBuild → sign → optional submit+wait. Checkbox `dryRun` default **checked** (stop after sign).

- [ ] **Step 3:** `npm i && npm run build` in browser-swap — expect Vite build success.

- [ ] **Step 4:** Commit `feat(sdk): add Freighter browser-swap example`

---

### Task 4: Docs + ApiReference

**Files:**
- Modify: `docs/integrator-guide.md`
- Modify: `docs/integrator-guide.zh-CN.md`
- Modify: `packages/sdk/README.md`
- Modify: `packages/frontend/src/components/docs/ApiReference.tsx`

- [ ] **Step 1:** Add “Browser (Freighter)” section pointing at `packages/sdk/examples/browser-swap`. Update endpoints table with new paths. Update SDK section for 0.2 methods.

- [ ] **Step 2:** Mirror zh-CN.

- [ ] **Step 3:** SDK README API table + examples link.

- [ ] **Step 4:** ApiReference: add Endpoint blocks for balance, submit_tx, tx_status (at minimum); PingTryIt where trivial.

- [ ] **Step 5:** Commit `docs: integrator DX for SDK 0.2 and wallet APIs`

---

### Task 5: Publish SDK 0.2.0

- [ ] **Step 1:** Ensure `packages/sdk/package.json` version is `0.2.0` and `dist/` built.

- [ ] **Step 2:** `./scripts/publish-sdk.sh` dry-run, then `./scripts/publish-sdk.sh --publish` (needs npm login).

- [ ] **Step 3:** `npm view @lumagg/sdk version` → `0.2.0`.

- [ ] **Step 4:** Commit any leftover version/README tweaks if needed; tag optional `sdk-v0.2.0`.

---

## Spec coverage

| Spec section | Task |
|--------------|------|
| OpenAPI paths | Task 1 |
| SDK methods + 0.2.0 | Task 2, 5 |
| browser-swap | Task 3 |
| Docs / ApiReference | Task 4 |
| Verification | Steps inside 2–5 |
| Out of scope (arb, pilots, official RPC) | Not scheduled |
