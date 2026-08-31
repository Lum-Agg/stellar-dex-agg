# D7 Path A: in-repo SDK validation

- Date: 2026-08-27
- SDK: `@lumagg/sdk` `0.3.0` from `packages/sdk`
- API: `https://api.lumagg.xyz`
- Scope: `quote → build_tx`; unsigned XDR only
- Account input: public mainnet G-address; no secret key

## Command

```bash
USER_G=GDXRRY4HHIERMJBY62B4YJ25V3YNTMEOG3CQRLRHJ3P57Q57CYSJLPI2 \
AMOUNT_STROOPS=10000000 \
npx --yes tsx packages/sdk/examples/quote-build.ts
```

## Output

```text
Quote OK
  expected_out: 1870724
  is_split: true
  legs: 2
build_tx OK
  execution: soroban
  fee stroops: 100000
  xdr prefix: AAAAAgAAAADvGOOHOgkWJDj2g8wnXa7w2bCONsUIridO39/DvxYklQAEXQQDy4BG...
```

The XDR is intentionally truncated because the acceptance criterion is proof
that a valid unsigned transaction was built, not publication of a
ledger-sensitive envelope. Re-run the command to generate a fresh full XDR.
