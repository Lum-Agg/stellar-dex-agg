# LumAgg 套利 Vault

**English:** [README.md](README.md)

**本合约仅用于 LumAgg 套利 bot，不是面向普通用户的金库或理财合约。**

普通 swap 请直接使用 [aggregator](../aggregator/) 的 `swap()` / `round_trip_swap()`，由用户钱包持币并签名即可。Vault 解决的是 **bot 运营** 问题：把交易本金集中存放在合约里，多个 bot 账号只需少量原生 XLM 付 gas，不必每个账号都备足 1800 XLM 等 float。

## 适用场景

| 场景 | 使用哪个合约 |
|------|----------------|
| 前端 / 钱包用户 swap | `aggregator.swap` |
| LumAgg arb bot 往返套利 | `vault.execute_round_trip` → 内部调用 `aggregator.round_trip_swap` |
| 手动一次性 round-trip | `aggregator.round_trip_swap`（caller 自己持币） |

## 执行流程

`execute_round_trip` 在 **单次合约调用** 内原子完成：

```text
caller ──approve(ceiling)──► token
vault ──amount_in──► caller ──round_trip_swap──► aggregator ──► DEX
                      ▲                              │
                      └──── base_total（本金+利润）──┘
vault ──transfer_from(base_total)──► caller   （走 allowance，金额不预签）
```

- 回收用固定额度的 `approve`（`i128::MAX`）+ `transfer_from`。allowance 的 **expiration 由调用参数传入**（bot 取 `latest_ledger + cushion`），不要在合约内用 `sequence()+N`——否则 simulate 与上链 ledger 差 1–2 会导致 SAC auth 对不上（`Unauthorized function call for address`）。
- 不对外暴露独立的 `withdraw`，避免 caller 单独提走资金。
- 利润随 `base_total` 一并回到 vault。
- `min_amount_out` 与 aggregator 相同，用于链上滑点/利润下限保护。

## 合约接口

| 函数 | 权限 | 说明 |
|------|------|------|
| `initialize(admin)` | 部署后一次 | 初始化 admin |
| `add_caller` / `remove_caller` | admin | 管理可执行套利的 bot 账号白名单 |
| `is_caller` | 只读 | 查询是否在白名单 |
| `deposit(from, token, amount)` | `from` 签名 | 向 vault 注资（通常 admin 或运营钱包） |
| `execute_round_trip(..., allowance_expiration_ledger)` | 白名单 caller 签名 | 唯一套利入口 |
| `admin_withdraw(token, to, amount)` | admin | 紧急提款 |
| `upgrade` | admin | 升级 WASM |

`execute_round_trip` 参数与 `aggregator.round_trip_swap` 对齐，额外需要传入 `aggregator` 合约地址，以及 bot 选定的 `allowance_expiration_ledger`（用于 reclaim 的 `approve`）。

## 与 arb bot 的配置

设置 `ARB_VAULT_CONTRACT` 后，`crates/arbitrage` 会构建 `vault.execute_round_trip` 交易（单 op），而不是直接调 aggregator：

```bash
ARB_VAULT_CONTRACT=C...        # 本 vault
ARB_AGGREGATOR_CONTRACT=C...   # LumAgg aggregator
ARB_CALLER_SECRETS=...         # 多个 bot 账号，各只需少量 XLM 作 fee
ARB_BUILD_TX=1
```

未设置 `ARB_VAULT_CONTRACT` 时，bot 仍走原来的 `aggregator.round_trip_swap`（caller 钱包自备本金）。

## 部署与运营

```bash
chmod +x contracts/vault/deploy.sh
ADMIN=admin ADMIN_G=G... CALLER=G... ./contracts/vault/deploy.sh
# 后续升级：VAULT=C... ./contracts/vault/upgrade.sh
```

1. 部署 vault WASM，`initialize(admin)`（或直接用 `deploy.sh`）。
2. `deposit` 将套利本金（如 XLM）打入 vault。
3. `add_caller` 为每个 bot 公钥授权。
4. 启动 arb bot；caller 账号 **不需要** 持有大额 trade token，只需原生 XLM 付 Soroban fee。

**并行注意：** 多个 caller 同时提交时，vault 余额需覆盖 `并发笔数 × amount_in`。例如 3 个 caller 各跑 500 XLM，vault 至少需 ~1500 XLM 可用余额。

## 构建与测试

```bash
cargo build -p vault-contract --target wasm32v1-none --release
cargo test -p vault-contract
```

## 安全说明

- 仅将 **可信任的 bot 热钱包** 加入 caller 白名单。
- Admin 密钥用于 `add_caller` / `admin_withdraw`，应妥善保管。
- 本合约 **不提供** 面向公众的存取理财功能；请勿引导普通用户向 vault 存款。
