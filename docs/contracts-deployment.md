# Smart Contract Deployment

LumAgg uses two main contracts:

| Contract | Required by | Purpose |
| --- | --- | --- |
| Aggregator | Swap API `/build_tx`, UI swaps, direct arbitrage | Atomically executes single, multi-hop, split, and round-trip routes |
| Vault | Optional for Arbitrage | Holds operator trading principal and allows approved callers to execute round trips |

The Aggregator is the public swap entrypoint. It does not permanently custody
user tokens. The Vault is an operator component, not a retail deposit or yield
product.

## Prerequisites

- Rust with the `wasm32v1-none` target.
- Stellar CLI and a configured network.
- A funded deployer identity.
- A separate admin identity whose key is protected for initialization and
  upgrades.

The repository currently uses Soroban SDK 22. Check the tested toolchain before
deploying and do not upgrade contract dependencies as part of a production
deployment without rerunning all contract tests.

Official references:

- [Stellar smart contract setup](https://developers.stellar.org/docs/build/smart-contracts/getting-started/setup)
- [Stellar CLI manual](https://developers.stellar.org/docs/tools/cli/stellar-cli)
- [Contract TTL maintenance](https://developers.stellar.org/docs/build/guides/conventions/extending-wasm-ttl)

## Configure Testnet

Use a named CLI network so every command targets the same RPC and passphrase:

```bash
stellar network add lumagg-testnet \
  --rpc-url https://soroban-testnet.stellar.org \
  --network-passphrase 'Test SDF Network ; September 2015'

stellar network health --network lumagg-testnet
stellar keys generate lumagg-admin --network lumagg-testnet
stellar keys fund lumagg-admin --network lumagg-testnet
export ADMIN_G=$(stellar keys address lumagg-admin)
```

For mainnet, create a separate network profile and use an existing funded
identity. Never reuse a testnet secret or place an admin secret in the
repository, shell history, environment examples, or systemd files.

## Test and Build

Run contract tests before producing deployment artifacts:

```bash
cargo test -p aggregator-contract -p vault-contract
```

Build optimized release WASM from the locked workspace:

```bash
mkdir -p build/contracts
stellar contract build --locked \
  --package aggregator-contract \
  --profile contract-release \
  --optimize \
  --out-dir build/contracts
stellar contract build --locked \
  --package vault-contract \
  --profile contract-release \
  --optimize \
  --out-dir build/contracts

openssl dgst -sha256 build/contracts/*.wasm
```

These commands were verified with Stellar CLI 25.2. Newer CLI versions also
optimize during `stellar contract build`; check `stellar contract build --help`
if the flag syntax changes. Always verify the resulting hash rather than
assuming two toolchains produce identical bytes.

Keep each deployed WASM artifact, SHA-256 hash, source commit, network, contract
ID, deployment transaction, and admin address in the release record.

## Deploy the Aggregator

Deploy and capture the returned contract ID:

```bash
export AGGREGATOR=$(stellar contract deploy \
  --wasm build/contracts/aggregator_contract.wasm \
  --source lumagg-admin \
  --network lumagg-testnet)

stellar contract invoke \
  --id "$AGGREGATOR" \
  --source lumagg-admin \
  --network lumagg-testnet \
  -- initialize --admin "$ADMIN_G"
```

`initialize` can run only once. Confirm the stored admin with a read-only
simulation:

```bash
stellar contract invoke \
  --id "$AGGREGATOR" \
  --source lumagg-admin \
  --network lumagg-testnet \
  --send=no \
  -- admin
```

Set this ID as `AGGREGATOR_CONTRACT` in the API deployment. `/quote` works
without it, but `/build_tx` requires it. For Arbitrage, set the same ID as
`ARB_AGGREGATOR_CONTRACT`.

The repository also includes `contracts/aggregator/deploy-testnet.sh`. It
refuses mainnet and is convenient for LumAgg development, but the explicit
commands above are easier for an external operator to audit.

## Deploy the Vault

The Vault is optional and should be deployed only by an Arbitrage operator:

```bash
export VAULT=$(stellar contract deploy \
  --wasm build/contracts/vault_contract.wasm \
  --source lumagg-admin \
  --network lumagg-testnet)

stellar contract invoke \
  --id "$VAULT" \
  --source lumagg-admin \
  --network lumagg-testnet \
  -- initialize --admin "$ADMIN_G"
```

Allowlist a bot caller and verify it:

```bash
export CALLER_G=G...

stellar contract invoke \
  --id "$VAULT" \
  --source lumagg-admin \
  --network lumagg-testnet \
  -- add_caller --caller "$CALLER_G"

stellar contract invoke \
  --id "$VAULT" \
  --source lumagg-admin \
  --network lumagg-testnet \
  --send=no \
  -- is_caller --caller "$CALLER_G"
```

Fund the Vault from a separate funded identity. `amount` uses the token's
integer units; XLM and most Stellar assets use seven decimals:

```bash
export FUNDER=lumagg-funder
export BASE_TOKEN=C...

stellar contract invoke \
  --id "$VAULT" \
  --source "$FUNDER" \
  --network lumagg-testnet \
  -- deposit --from "$(stellar keys address "$FUNDER")" \
  --token "$BASE_TOKEN" --amount 1000000000
```

After verifying the balance and caller allowlist, configure
`ARB_VAULT_CONTRACT=$VAULT` and follow the staged
[Arbitrage deployment](arbitrage-deployment.md). Do not enable transaction
submission during contract deployment validation.

## Upgrade

An upgrade changes the code behind the existing contract ID. Test the new WASM
against a fresh testnet deployment before touching mainnet. Upload the exact
artifact, record its hash, then invoke the admin-only `upgrade` function:

```bash
export NEW_WASM=build/contracts/aggregator_contract.wasm
export NEW_WASM_HASH=$(openssl dgst -sha256 "$NEW_WASM" | awk '{print $NF}')

stellar contract upload \
  --wasm "$NEW_WASM" \
  --source lumagg-admin \
  --network lumagg-testnet

stellar contract invoke \
  --id "$AGGREGATOR" \
  --source lumagg-admin \
  --network lumagg-testnet \
  -- upgrade --new_wasm_hash "$NEW_WASM_HASH"
```

The same process applies to the Vault by changing the artifact and contract ID.
Keep the previous WASM and a tested rollback artifact. An on-chain upgrade is a
state transition and cannot be undone with Git alone.

## TTL Maintenance

LumAgg intentionally keeps TTL extension outside the contracts. Stellar stores
the contract instance and WASM as separate ledger entries, and both can become
archived if they are not extended.

Use the exact deployed WASM or its recorded hash:

```bash
CONTRACT_ID="$AGGREGATOR" \
WASM=build/contracts/aggregator_contract.wasm \
SOURCE=lumagg-admin \
NETWORK=lumagg-testnet \
LEDGERS_TO_EXTEND=2073600 \
  ./scripts/extend-contract-ttl.sh
```

At an average five-second ledger close, `2073600` ledgers is approximately 120
days. Run the operation well before the remaining TTL reaches 30 days and
monitor the resulting transactions. Network limits can change, so confirm the
accepted range with the current Stellar CLI and RPC before scheduling it.

The helper extends the contract instance and WASM by default. Vault allowlist
entries use persistent storage and have their own TTL; reading or rewriting an
entry does not automatically extend it. Generate the base64 XDR for each
`Caller(Address)` storage key and pass the keys to the same helper:

```bash
CONTRACT_ID="$VAULT" \
WASM=build/contracts/vault_contract.wasm \
SOURCE=lumagg-admin \
NETWORK=lumagg-testnet \
PERSISTENT_KEY_XDRS='<caller-1-key-xdr>,<caller-2-key-xdr>' \
  ./scripts/extend-contract-ttl.sh
```

The Stellar CLI requires `--key-xdr` for enum keys containing an address. See
the official
[persistent-entry CLI guide](https://developers.stellar.org/docs/tools/cli/cookbook/extend-contract-storage)
for the key format. Continue to verify `is_caller` before enabling live
submissions; an archived caller entry must be restored before contract code can
read or recreate it.

## Production Checklist

- Test the exact WASM and record its hash before deployment.
- Verify network, RPC, admin address, and contract ID at every command.
- Keep deployer, admin, funder, and bot caller roles separate where practical.
- Confirm Aggregator `admin` and Vault caller authorization on-chain.
- Start API and Arbitrage in non-submitting modes before a live swap.
- Monitor and externally extend instance, WASM, and Vault caller TTLs.
- Protect upgrade and emergency-withdraw authority with an operational key
  policy appropriate to the funds at risk.
