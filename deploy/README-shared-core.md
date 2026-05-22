# Shared stellar-core (config only)

## Layout (default)

```
stellar-rpc (:8003)
  └── sole stellar-core (:11626 / :11628)
stellar-horizon     → disabled (Classic DEX uses https://horizon.stellar.org)
lumagg worker/api   → RPC_URL=http://127.0.0.1:8003 when getHealth is healthy
```

- **Do not** run `stellar-core.service` or `stellar-horizon.service` alongside this profile.
- Captive config: `/etc/stellar/stellar-captive-mainnet.cfg`
- RPC config: `/etc/stellar/soroban-rpc.toml`

## Apply on server

```bash
sudo ./deploy/setup_shared_stellar_core.sh
```

## Disable Horizon only

```bash
sudo systemctl stop stellar-horizon
sudo systemctl disable stellar-horizon
```

Postgres data under `horizon` DB is left on disk; remove separately if you need disk back.

## Reset stuck RPC ingest

```bash
sudo ./deploy/reset_stellar_rpc_ingest.sh
```

## Files

| File | Role |
|------|------|
| `soroban-rpc.toml` | RPC TOML (owned core, ports 11626/11628) |
| `stellar-captive-mainnet.cfg` | Captive core template |
| `reset_stellar_rpc_ingest.sh` | Wipe sqlite + rpc-captive, restart RPC |
