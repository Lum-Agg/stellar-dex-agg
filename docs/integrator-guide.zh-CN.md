# LumAgg 集成指南

本指南面向希望接入 LumAgg 公共 REST API 的钱包、DApp 和交易机器人。

**线上 API：** https://api.lumagg.xyz  
**OpenAPI：** [openapi.yaml](./openapi.yaml) · **在线文档：** https://lumagg.xyz/docs  
**基准测试：** [scf-benchmark-results.md](./scf-benchmark-results.md) · [scf-venue-comparison.md](./scf-venue-comparison.md)

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

# 3）构建未签名 XDR
# 将报价中的 sub_routes 放入 POST /api/v1/build_tx 请求体，详情见 OpenAPI。
```

完整流程：

**`GET /quote`** → 将 `sub_routes` 传给 **`POST /build_tx`** → 钱包签署 XDR → 通过 Soroban RPC 或 Horizon 提交交易。

### 外部集成者一键测试（推荐）

先克隆仓库并进入项目目录，然后执行：

```bash
chmod +x scripts/integrator-smoke.sh
USER_G=G你的已激活主网地址 ./scripts/integrator-smoke.sh
```

`USER_G` 必须是一个已经存在于 Stellar 主网、拥有 sequence number 的 G 地址；账户中有少量 XLM 即可。脚本只会构建**未签名交易**，不会要求私钥，也不会提交交易。成功时会输出 `unsigned_tx_xdr` 的前缀。

如需保存测试结果作为 grant 验收证据：

```bash
OUT=./evidence/pilot-friend USER_G=G... ./scripts/integrator-smoke.sh
```

请将以下内容反馈给 LumAgg 团队：

- 测试是否成功；
- 使用的操作系统和大致环境；
- 文档中是否有不清楚或缺失的步骤；
- `evidence/pilot-friend` 目录中的输出（请勿发送私钥或助记词）。

也可以使用 SDK 示例：

```bash
USER_G=G... npx tsx packages/sdk/examples/quote-build.ts
```

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

**合作伙伴 Key 申请方式：** 通过 GitHub Issue 或 grant 联系方式联系 LumAgg 团队。服务端使用以下环境变量配置 Key：

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
| GET | `/api/v1/balance` | 查询单个 SAC 余额 |
| GET | `/api/v1/balances` | 批量查询常用 Token 余额 |

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

## 6. 差异化能力证据

在本地重新运行报价基准测试：

```bash
./scripts/scf-benchmark.sh
LUMAGG_PREFER_SOROBAN=1 SOROSWAP_API_KEY=sk_... ./scripts/scf-benchmark.sh
```

关于 LumAgg 与 Stellar Broker 的流动性源覆盖矩阵和拆单路由说明，请参阅 [scf-venue-comparison.md](./scf-venue-comparison.md)。

## 7. npm SDK（Tranche 2）

代码目录：`packages/sdk`  
计划发布包名：`@lumagg/sdk`

```bash
npx tsx packages/sdk/examples/quote-build.ts
npx tsx packages/sdk/examples/basic-usage.ts
```

详情参见 [packages/sdk/README.md](../packages/sdk/README.md)。

## 8. 链上统计

当 API 服务挂载 indexer 数据库后，可查询公开统计：

```bash
curl -s https://api.lumagg.xyz/api/v1/stats | jq .
```

示例导出：[sample-indexer-export.json](./sample-indexer-export.json) · 数据管线：[analytics-indexer.md](./analytics-indexer.md)。

## 9. 原子套利 Operator

自行部署 vault 和套利机器人，请参阅 [arb-operator.md](./arb-operator.md)。
