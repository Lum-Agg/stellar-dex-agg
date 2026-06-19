# Third-party reference (optional, not in git)

Upstream DEX repos for **local development only** — reading contract layouts, events, and mainnet address manifests. They are **not** Cargo dependencies: `lumagg` builds and deploys without this folder (`deploy_server.sh` excludes `thirdparty`).

Clone what you need into this directory (names match LumAgg adapter sources):

| Directory | Repository | Used for |
|-----------|------------|----------|
| `aquarius-amm` | [AquaToken/soroban-amm](https://github.com/AquaToken/soroban-amm) | Aquarius xy=k, stable, CLMM storage |
| `comet-contracts-v1` | [CometDEX/comet-contracts-v1](https://github.com/CometDEX/comet-contracts-v1) | Comet weighted pool math / factory |
| `phoenix-contracts` | [Phoenix-Protocol-Group/phoenix-contracts](https://github.com/Phoenix-Protocol-Group/phoenix-contracts) | Phoenix pool interface |
| `soroswap` | [soroswap/core](https://github.com/soroswap/core) | Router/factory IDs (`public/mainnet.contracts.json`) |
| `sushiswap-stellar-interface-fork` | [hyplabs/sushiswap-stellar-interface-fork](https://github.com/hyplabs/sushiswap-stellar-interface-fork) | Sushi V3 bindings / pool layout |

## Quick setup

```bash
mkdir -p thirdparty
git clone https://github.com/soroswap/core thirdparty/soroswap
git clone https://github.com/AquaToken/soroban-amm thirdparty/aquarius-amm
git clone https://github.com/Phoenix-Protocol-Group/phoenix-contracts thirdparty/phoenix-contracts
git clone https://github.com/CometDEX/comet-contracts-v1 thirdparty/comet-contracts-v1
git clone https://github.com/hyplabs/sushiswap-stellar-interface-fork thirdparty/sushiswap-stellar-interface-fork
```

## Pinned revisions (last used during LumAgg integration)

Check out these commits when diffing against upstream; newer upstream may still be compatible.

| Path | Commit |
|------|--------|
| `aquarius-amm` | `c4d842de3108a23a4a1107b7d54c357bae45f962` |
| `comet-contracts-v1` | `ef4cbfad0a35202ad267c14d163d2f362995a8d3` |
| `phoenix-contracts` | `3af5ffafed41f1a5444f79ab1642cf9a7f0f59bc` |
| `soroswap` | `bb90a65556d8eee0dc698ac75de0f280e547fedc` |
| `sushiswap-stellar-interface-fork` | `76bc13357652ddca628c990dfe2aad9046fc4090` |

Example:

```bash
git -C thirdparty/soroswap checkout bb90a65556d8eee0dc698ac75de0f280e547fedc
```

## Why not git submodules?

Reference trees are large and optional. Keeping them out of the main repo avoids broken gitlink state, keeps `git clone` fast, and matches production (server build never needs `thirdparty/`). Pin commits here when you refresh adapters.
