# LumAgg 集成指南

本指南面向希望接入 LumAgg 公共 REST API 的钱包、DApp 和交易机器人。

**线上 API：** https://api.lumagg.xyz  
**OpenAPI：** [openapi.yaml](./openapi.yaml) · **文档：** https://lumagg.gitbook.io/  
**API 参考：** [api-reference.md](./api-reference.md)

## 1. 获取报价 → 构建交易 → 签名

```bash
API=https://api.lumagg.xyz
XLM=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA
USDC=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75

# 1）获取报价（1 XLM → USDC）
curl -sG "$API/api/v1/quote" \
  --data-urlencode "token_in=$XLM" \
  --data-urlencode "token_out=$USDC" \
  --data-urlencode "amount_in=10000000" \
  --data-urlencode "slippage=0.5"

# 2）仅使用 Soroban 流动性源报价
# 排除 Classic SDEX，便于与 Soroswap API 公平比较
curl -sG "$API/api/v1/quote" \
  --data-urlencode "token_in=$XLM" \
  --data-urlencode "token_out=$USDC" \
  --data-urlencode "amount_in=10000000" \
  --data-urlencode "prefer_soroban=1"

# 3）构建未签名 XDR — 将报价中的 sub_routes 放入 POST /api/v1/build_tx
curl -sX POST "$API/api/v1/build_tx" \
  -H 'Content-Type: application/json' \
  -d '{
    "user": "G你的已激活主网地址",
    "token_in": "'"$XLM"'",
    "token_out": "'"$USDC"'",
    "amount_in": "10000000",
    "slippage": 0.5,
    "sub_routes": []
  }'
```

将 `sub_routes` 替换为 `/quote` 返回的数组。完整 schema 见 [OpenAPI](./openapi.yaml)。

完整流程：

**`GET /quote`** → 将 `sub_routes` 传给 **`POST /build_tx`** → 钱包签署 XDR → 通过 **`POST /api/v1/submit_tx`**（由 LumAgg 代理 Soroban RPC）或任意同网络 Soroban RPC 提交。

### 一键冒烟测试

先克隆仓库并进入项目目录，然后执行：

```bash
chmod +x scripts/integrator-smoke.sh
USER_G=G你的已激活主网地址 ./scripts/integrator-smoke.sh

# 可选：将 JSON 输出保存备查
OUT=./tmp/smoke USER_G=G... ./scripts/integrator-smoke.sh
```

`USER_G` 必须是一个已经存在于 Stellar 主网、拥有 sequence number 的 G 地址；账户中有少量 XLM 即可。脚本只会构建**未签名交易**，不会要求私钥，也不会提交交易。成功时会输出 `unsigned_tx_xdr` 的前缀。

换入经典资产 SAC（如 USDC/EURC）时，账户需**已有**对应 trustline，否则 simulate 会失败。请先在 Freighter 等钱包中添加 trustline（约 0.5 XLM 准备金）。可通过 `/api/v1/balance` 与 `/api/v1/balances` 返回的 `has_trustline` 字段检测（与余额查询共用 SAC simulate，不额外请求 Horizon）。

也可以使用 SDK 示例：

```bash
USER_G=G... npx tsx packages/sdk/examples/quote-build.ts
```

### 浏览器（Freighter）— 完整签名与提交

CLI smoke 只到未签名 XDR。完整链路（quote → build → Freighter 签名 → `submit_tx` → `tx_status`）：

```bash
cd packages/sdk && npm run build
cd examples/browser-swap && npm install && npm run dev
```

打开 Vite 地址，连接 Freighter（Public）；默认勾选 **Dry-run** 可在签名后停止，取消勾选则会在主网提交。详见 [`packages/sdk/examples/browser-swap/README.md`](../packages/sdk/examples/browser-swap/README.md)。

## 2. `prefer_soroban`

| 值 | 行为 |
|---|---|
| 省略或设为 `0` | 在 **Soroban AMM + Classic SDEX** 中寻找最优价格 |
| `1` | **仅使用 Soroban**，不返回 PathPayment / SDEX 路径 |

当钱包无法在同一流程中签署 Classic PathPayment，或需要与仅使用 Soroban 的聚合器进行比较时，可设置 `prefer_soroban=1`。

Soroswap API 可设置 `protocols: ["soroswap","phoenix","aqua"]`，即省略 `"sdex"`，实现相同的比较条件。参见 [Soroswap API 文档](https://docs.soroswap.finance/soroswap-api)。

## 3. 速率限制和 API Key

| 级别 | 限制 | 认证方式 |
|---|---|---|
| 匿名用户 | 每个 IP 每秒 10 次请求 | 无 |
| 合作伙伴 | 每个 Key 每秒 60 次请求 | 请求头 `X-API-Key: <key>` |

请求超过限制时返回 HTTP `429`。服务端配置了合作伙伴 Key 后，无效的 `X-API-Key` 会返回 `401`。

**合作伙伴 Key 申请方式：** 通过 [GitHub Issue](https://github.com/Lum-Agg/stellar-dex-agg/issues) 或联系 LumAgg 团队。服务端使用以下环境变量配置 Key：

```bash
LUMAGG_PARTNER_API_KEYS=key_one,key_two
```

## 4. API 端点

| 方法 | 路径 | 用途 |
|---|---|---|
| GET | `/api/v1/health` | 存活检查 |
| GET | `/api/v1/tokens` | 可路由 Token + **自托管** Logo URL |
| GET | `/logos/{file}` | 静态 Token Logo 文件（`image/png|jpeg|webp|svg+xml`） |
| GET | `/api/v1/quote` | 获取最优路由 |
| POST | `/api/v1/build_tx` | 构建未签名 XDR |
| GET | `/api/v1/balance` | 查询单个 SAC 余额（可知时返回 `has_trustline`） |
| GET | `/api/v1/balances` | 批量查询常用 Token 余额 + 每 token 的 `has_trustline` |
| GET | `/api/v1/account` | 账户 sequence（Soroban RPC `getLedgerEntries`） |
| GET | `/api/v1/classic_asset` | 将 SAC `C…` 解析为 classic `code` / `issuer` |
| GET | `/api/v1/ledger/latest` | 最新已关闭 ledger |
| POST | `/api/v1/submit_tx` | 提交已签名 XDR（`{ "signed_tx_xdr": "..." }`）— 快速入队 |
| GET | `/api/v1/tx_status` | 在 `submit_tx` 后轮询上链状态（`confirmed` 仅当 SUCCESS） |
| GET | `/api/v1/prices` | 批量查询最新 USDC 标价 |
| GET | `/api/v1/prices/history` | 查询采样价格历史（图表） |
| GET | `/api/v1/orders` | 钱包限价单（indexer DB） |
| POST | `/api/v1/orders/build_create` | `create_limit` 未签名 XDR |
| POST | `/api/v1/orders/build_cancel` | `cancel` 未签名 XDR |

`/api/v1/tokens[].logo` 在 enrichment 完成前可能为空；完成后为自托管绝对 URL：

```text
https://api.lumagg.xyz/logos/
```

可选字段 `logo_kind`：
- `"official"` — 来自 SEP-42 列表（Soroswap / LOBSTR / StellarExpert Top50），按原格式自托管（PNG/JPEG/WebP/GIF/SVG）
- `"fallback"` — 无官方图标时本地生成的字母头像

请不要依赖第三方图床展示 Token 图标。

## 5. 执行模式

- **Soroban：** `build_tx` 返回 `execution: "soroban"`，交易包含一次 `aggregator.swap` 调用，支持多跳和拆单。
- **Classic：** 当报价仅使用 SDEX 时，返回 `execution: "classic"`，交易使用 `PathPaymentStrictSend`。
- **不支持混合执行：** Classic 和 Soroban 路径不能合并到同一笔 Stellar 交易中。

## 6. 复现报价基准测试

当你想对比路由质量（例如仅 Soroban vs 多 venue）时再跑这些脚本；日常集成不需要。

```bash
./scripts/scf-benchmark.sh
LUMAGG_PREFER_SOROBAN=1 SOROSWAP_API_KEY=sk_... ./scripts/scf-benchmark.sh
```

Venue 覆盖说明：[Performance / venue comparison](./scf-venue-comparison.md)。生产集成若需要仅 Soroban 范围，一般在 `/quote` 上设 `prefer_soroban=1` 即可。

## 7. npm SDK

已发布：[`@lumagg/sdk`](https://www.npmjs.com/package/@lumagg/sdk) `0.2.0`（`packages/sdk`）。

| SDK 方法 | REST |
|----------|------|
| `quote` / `buildTx` / `quoteAndBuild` | `/quote`, `/build_tx` |
| `getBalance` / `getBalances` | `/balance`, `/balances` |
| `getAccount` / `getClassicAsset` / `getLatestLedger` | `/account`, `/classic_asset`, `/ledger/latest` |
| `submitTx` / `getTxStatus` / `waitForTx` | `/submit_tx`, `/tx_status` |
| `listTokens` / `getStats` / `listSwaps` / orders / prices | 见 OpenAPI |

```bash
USER_G=G... npx tsx packages/sdk/examples/quote-build.ts
npx tsx packages/sdk/examples/basic-usage.ts
# Freighter 端到端：
cd packages/sdk/examples/browser-swap && npm run dev
```

详情参见 [packages/sdk/README.md](../packages/sdk/README.md)。

## 8. 链上统计

当 API 服务挂载 indexer 数据库后，可查询公开统计：

```bash
curl -s https://api.lumagg.xyz/api/v1/stats | jq .
```

示例导出：[sample-indexer-export.json](./sample-indexer-export.json) · 数据管线：[analytics-indexer.md](./analytics-indexer.md)。

### 钱包 Swap 历史

查询某个 Stellar 账户最近的 LumAgg 聚合器调用记录（与 `/stats` 使用同一 indexer 数据库）：

```bash
curl -s "https://api.lumagg.xyz/api/v1/swaps?user=G...&limit=20" | jq .
```

响应中的 `data.swaps[]` 包含 `tx_hash`、Token 数量、`status` 和 `is_split` 等字段。无历史记录时仍返回 `200`，`swaps` 为空数组。服务端需配置 TOML 的 `[indexer]` 段，否则返回 `503`。

### 限价单

查询 open 限价单，并构建 create/cancel 未签名 XDR（order-escrow 合约）：

```bash
curl -s "https://api.lumagg.xyz/api/v1/orders?user=G...&status=open" | jq .

curl -sX POST "https://api.lumagg.xyz/api/v1/orders/build_create" \
  -H 'Content-Type: application/json' \
  -d '{
    "user": "G...",
    "token_in": "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA",
    "token_out": "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
    "amount_in": "10000000",
    "limit_out_per_in_e7": "20000000",
    "expires_ledger": 12345678
  }' | jq .

curl -sX POST "https://api.lumagg.xyz/api/v1/orders/build_cancel" \
  -H 'Content-Type: application/json' \
  -d '{"user": "G...", "order_id": 1}' | jq .
```

`GET /orders` 与 `/swaps` 共用 indexer SQLite（`indexer.db_path`）。build 接口需配置 `features.escrow_contract`。响应字段与 `build_tx` 一致：`unsigned_tx_xdr`、`fee`、`execution`、`num_operations`、`contract`。SDK 方法：`listOrders`、`buildCreateOrder`、`buildCancelOrder`。

**限价单环境变量（api-server 运维）：**

| 变量 | 说明 |
|------|------|
| `indexer.db_path` | 含 `limit_orders` 表的 SQLite（列表接口必需） |
| `ESCROW_CONTRACT` | 已部署的 order-escrow 合约 id（build 接口必需） |

### Token 价格与图表历史

用于 Portfolio 估值和简易 sparkline 的 USDC 标价（来自报价引擎）：

```bash
XLM=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA
USDC=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75

# 批量最新标价（最多 50 个 id）
curl -sG "https://api.lumagg.xyz/api/v1/prices" \
  --data-urlencode "ids=$XLM,$USDC" | jq .

# 采样历史（默认 range=24h）
curl -sG "https://api.lumagg.xyz/api/v1/prices/history" \
  --data-urlencode "id=$XLM" \
  --data-urlencode "range=7d" | jq .
```

`GET /prices` 返回 `data.prices[]`，字段包括 `id`、`price_usdc`、`ts`、`via`（`usdc` 或 `xlm`）。无历史 tick 时会按需报价一次。无法定价的 Token 不会出现在结果中。

`GET /prices/history` 返回 `data.points[]`（`ts`、`price_usdc`）。无数据时仍返回 `200`，`points` 为空数组。`range` 仅支持 `24h` 或 `7d`。

**采样器环境变量（api-server 运维）：**

| 变量 | 说明 |
|------|------|
| `PRICE_DB_PATH` | 采样 tick 的 SQLite 路径（history 与后台采样器均依赖此项） |
| `PRICE_SAMPLER` | 设为 `0` 关闭后台采样（默认：配置 `PRICE_DB_PATH` 后启用） |
| `PRICE_SAMPLE_SECS` | 采样间隔（秒），默认 `600` |
| `PRICE_SAMPLE_TOKEN_LIMIT` | 除优先列表外，从 registry 采样的 Token 数量上限，默认 `30` |
| `PRICE_RETENTION_DAYS` | 可选正整数，删除早于 N 天的 tick（默认永久保留） |

## 9. 原子套利 Operator

自行部署 vault 和套利机器人，请参阅 [arb-operator.md](./arb-operator.md)。
