# Implementation Tasks: Stellar DEX Aggregator

## 现有代码复用分析

基于 `stellar-arb` 项目的代码审查，以下模块可直接复用或改造：

| 现有模块 | 复用方式 | 对应新模块 |
|----------|----------|------------|
| `defi/protocol.rs` (Protocol enum) | 直接复用，扩展 Comet | DexAdapter trait 的 ProtocolType |
| `defi/operation.rs` (PoolOperations trait) | 改造为 DexAdapter trait | 适配器层 |
| `defi/protocols/aquarius.rs` | 改造，增加 build_swap_operation | Aquarius 适配器 |
| `defi/protocols/soroswap.rs` | 改造，增加 build_swap_operation | Soroswap 适配器 |
| `defi/protocols/sdex.rs` + `sdex_orderbook.rs` | 改造合并 | Classic DEX 适配器 |
| `arb/pool_manager.rs` (coin_to_coin_to_pool_id 图) | 改造为 TokenGraph | PathFinder |
| `arb/item.rs` (asset_to_contract_hash, parse_asset_for_pathpayment) | 直接复用 | TransactionBuilder |
| `arb/item.rs` (build_amm_steps, build_and_sign_tx) | 改造 | TransactionBuilder |
| `arb/strategy/arb_item_worker.rs` (local_simulate_arb) | 改造为报价计算 | QuoteManager |
| `stellar_client.rs` | 复用 RPC 交互逻辑 | Simulator |
| `types.rs`, `config.rs` | 部分复用 | 配置管理 |

---

## Task 1: 项目脚手架和核心类型定义

- [ ] 初始化 Rust workspace（`Cargo.toml` workspace 包含 `crates/router-engine`、`crates/aggregator-contract`、`packages/sdk`、`apps/frontend`）
- [ ] 定义核心类型：`TokenId`、`TradingPair`、`Quote`、`FeeInfo`、`Path`、`SwapOperation`、`OptimalRoute`、`SubOrder`
- [ ] 从 `stellar-arb` 迁移 `asset_to_contract_hash`、`parse_asset_for_pathpayment`、`compute_sac_contract_hash` 工具函数
- [ ] 配置管理：RPC URL、Horizon URL、网络 passphrase、桥接代币列表、合约地址
- [ ] 添加依赖：`stellar-xdr`、`stellar-strkey`、`soroban-client`、`stellar-rpc-client`、`axum`、`tokio`、`serde`、`anyhow`、`async-trait`、`reqwest`

**对应需求**: 基础架构  
**可复用代码**: `stellar-arb/src/types.rs`、`stellar-arb/src/config.rs`、`stellar-arb/src/arb/item.rs`（工具函数部分）

---

## Task 2: DexAdapter trait 和 Soroswap 适配器

- [ ] 定义 `DexAdapter` trait（`id`、`name`、`protocol_type`、`get_available_pairs`、`get_quote`、`build_swap_operation`、`health_check`）
- [ ] 实现 `SoroswapAdapter`：
  - [ ] `get_available_pairs`：查询 Soroswap Factory 合约获取所有 pair 地址，或调用 Soroswap API
  - [ ] `get_quote`：基于 reserves 计算 constant product 报价（从 `stellar-arb/src/defi/protocols/soroswap.rs` 迁移 `quote_exact_in` 逻辑）
  - [ ] `build_swap_operation`：生成 `SorobanInvoke`（调用 Soroswap Router 的 `swap_exact_tokens_for_tokens`）
  - [ ] `health_check`：验证 RPC 连通性
- [ ] 单元测试：mock reserves 验证报价计算正确性

**对应需求**: Req 1, Req 8  
**可复用代码**: `stellar-arb/src/defi/protocols/soroswap.rs`

---

## Task 3: Aquarius 适配器

- [ ] 实现 `AquariusAdapter`：
  - [ ] `get_available_pairs`：调用 Aquarius API (`amm-api.aqua.network`) 获取池列表
  - [ ] `get_quote`：基于 reserves 计算报价（从 `stellar-arb/src/defi/protocols/aquarius.rs` 迁移），支持 constant_product 和 stable 池
  - [ ] `build_swap_operation`：生成 `SorobanInvoke`（调用 Aquarius Router 的 `swap` 或 `swap_chained`）
  - [ ] `health_check`
- [ ] 单元测试

**对应需求**: Req 1, Req 8  
**可复用代码**: `stellar-arb/src/defi/protocols/aquarius.rs`

---

## Task 4: Stellar Classic DEX 适配器

- [ ] 实现 `ClassicDexAdapter`：
  - [ ] `get_available_pairs`：通过 Horizon API 查询活跃交易对（或使用预配置的主要交易对列表）
  - [ ] `get_quote`：调用 Horizon `/order_book` 端点获取深度，计算实际成交价格；或使用 AMM 池 reserves 计算
  - [ ] `build_swap_operation`：生成 `ClassicPathPayment`（`path_payment_strict_send`）
  - [ ] `health_check`：验证 Horizon API 连通性
- [ ] 单元测试

**对应需求**: Req 1, Req 8  
**可复用代码**: `stellar-arb/src/defi/protocols/sdex.rs`、`stellar-arb/src/defi/protocols/sdex_orderbook.rs`

---

## Task 5: Comet 适配器

- [ ] 实现 `CometAdapter`：
  - [ ] `get_available_pairs`：查询 Comet 合约获取加权池列表
  - [ ] `get_quote`：基于 Balancer 加权数学公式计算报价（`out = reserve_out * (1 - (reserve_in / (reserve_in + amount_in))^(weight_in/weight_out))`）
  - [ ] `build_swap_operation`：生成 `SorobanInvoke`（调用 Comet 池的 `swap` 函数）
  - [ ] `health_check`
- [ ] 单元测试

**对应需求**: Req 1, Req 8  
**可复用代码**: 无直接复用（`stellar-arb` 中没有 Comet），需新写

---

## Task 6: TokenGraph 和 PathFinder（路径发现）

- [ ] 实现 `TokenGraph`：
  - [ ] 邻接表结构（`HashMap<TokenId, Vec<Edge>>`）
  - [ ] `add_edge`：双向添加交易对
  - [ ] `remove_edges_by_source`：按 DEX 源移除边
  - [ ] `find_paths`：BFS 搜索所有路径（max_hops=4）
- [ ] 实现 `PathFinder`：
  - [ ] `update_from_adapter`：从适配器获取交易对更新图
  - [ ] `discover_paths`：调用 BFS，优先通过桥接代币（XLM、USDC）搜索
  - [ ] 路径缓存 + `invalidate` 失效机制
- [ ] 桥接代币配置（XLM、USDC、EURC 等高流动性代币）
- [ ] 单元测试：构造各种图结构验证 BFS 正确性、跳数约束、缓存失效

**对应需求**: Req 2, Req 11  
**可复用代码**: `stellar-arb/src/arb/pool_manager.rs`（`coin_to_coin_to_pool_id` 图结构逻辑）

---

## Task 7: 报价管理器（QuoteManager）

- [ ] 实现 `QuoteManager`（门面模式，协调各组件）：
  - [ ] `register_adapter` / `unregister_adapter`：动态注册/移除适配器
  - [ ] `get_quote`：
    1. 调用 PathFinder 发现路径
    2. 并行查询各适配器获取报价（带 5s 超时）
    3. 过滤无效/超时报价
    4. 对每条路径评估输出（调用 `quote_exact_in` 逐跳计算）
    5. 选择最优路径
  - [ ] 报价归一化：统一格式（output amount、price impact、fee）
  - [ ] 超时处理：单个 DEX 源 5s 超时后排除
  - [ ] 错误处理：无效报价丢弃 + 日志
- [ ] 单元测试：mock 适配器验证并行查询、超时排除、最优选择

**对应需求**: Req 1, Req 2  
**可复用代码**: `stellar-arb/src/arb/strategy/arb_item_worker.rs`（`local_simulate_arb` 逐跳报价逻辑）

---

## Task 8: 拆单引擎（SplitEngine）

- [ ] 实现 `SplitEngine`：
  - [ ] `should_split`：判断单路径 price_impact > 100 bps (1%) 时触发拆单
  - [ ] `optimize`：
    1. 收集所有候选路径的报价函数（amount → output 的映射）
    2. 对每条路径用不同比例（10%, 20%, ..., 90%）模拟输出
    3. 贪心 + 二分搜索找到最优分配比例
    4. 验证拆单结果优于最优单路径
    5. 最多拆分为 5 个子订单
  - [ ] 返回 `OptimalRoute`（含 `sub_orders`、`improvement_bps`）
- [ ] Price impact 计算：对 AMM 池使用 `amount_in / reserve_in` 近似
- [ ] 单元测试：各种流动性分布下验证拆单改善

**对应需求**: Req 3  
**可复用代码**: `stellar-arb/src/arb/strategy/arb_item_worker.rs`（`build_candidate_inputs`、`scan_candidate_inputs` 的离散扫描思路）

---

## Task 9: 交易构建器（TransactionBuilder）

- [ ] 实现 `TransactionBuilder`：
  - [ ] `build`：根据 OptimalRoute 生成原子交易
    - 纯 Soroban 路径：调用聚合合约 `aggregate_swap`
    - 纯 Classic DEX 路径：生成 `PathPaymentStrictSend` 操作
    - 混合路径：Soroban invoke + Classic PathPayment 组合
  - [ ] 滑点保护：根据 `slippage_bps` 计算 `min_amount_out`
  - [ ] `simulate`：调用 Stellar RPC `simulateTransaction`
  - [ ] 返回 `UnsignedTx`（XDR + hash + fee 估算）
- [ ] 混合交易构建逻辑（从 `stellar-arb` 的 `build_and_sign_mixed_tx` 改造，去掉签名部分）
- [ ] 单元测试 + 集成测试（Testnet）

**对应需求**: Req 4, Req 5, Req 12  
**可复用代码**: `stellar-arb/src/arb/item.rs`（`build_amm_steps`、`build_and_sign_tx`、`build_pure_sdex_tx`、`build_and_sign_mixed_tx` 的交易构建逻辑，去掉签名和套利特定逻辑）

---

## Task 10: REST API（Axum）

- [ ] 实现 API 路由：
  - [ ] `GET /api/v1/quote`：接收 token_in、token_out、amount_in、slippage_tolerance → 返回 QuoteResponse
  - [ ] `POST /api/v1/swap`：接收 SwapRequest → 返回未签名交易 XDR + 模拟结果
  - [ ] `POST /api/v1/simulate`：接收交易 XDR → 返回模拟结果
  - [ ] `GET /api/v1/tokens`：返回支持的代币列表
  - [ ] `GET /api/v1/health`：健康检查（各适配器状态）
- [ ] 请求验证：token 格式、amount 范围、slippage 范围 (0.01-50)
- [ ] 错误响应格式统一
- [ ] CORS 配置（前端跨域）
- [ ] 集成测试

**对应需求**: Req 6, Req 7  
**可复用代码**: 无直接复用（新增 HTTP 层）

---

## Task 11: Soroban 聚合合约

- [ ] 在 `crates/aggregator-contract` 中实现合约：
  - [ ] `initialize`：设置 admin
  - [ ] `register_protocol` / `remove_protocol`：管理支持的 DEX 协议
  - [ ] `aggregate_swap`：
    1. 验证 caller 授权
    2. 从 caller 转入 token_in
    3. 按顺序执行 swap_ops（cross-contract call 到各 DEX）
    4. 验证最终 token_out 余额 >= min_amount_out
    5. 将 token_out 转回 caller
    6. 任何步骤失败则 revert
  - [ ] 事件发射：每个 swap 操作发射 `SwapEvent`
- [ ] 支持的协议调用：
  - [ ] Soroswap：调用 `swap_exact_tokens_for_tokens`
  - [ ] Aquarius：调用 `swap` 或 `swap_chained`
  - [ ] Comet：调用 `swap`
- [ ] 合约测试（soroban-sdk test framework）
- [ ] 部署脚本（Testnet）

**对应需求**: Req 9  
**可复用代码**: `stellar-arb/arb-contract/`（现有套利合约的 cross-contract call 模式）

---

## Task 12: TypeScript SDK

- [ ] 初始化 `packages/sdk`（TypeScript，发布为 npm 包）
- [ ] 实现 `StellarAggregator` class：
  - [ ] `getQuote(params)` → 调用 REST API `/api/v1/quote`
  - [ ] `buildSwap(params)` → 调用 REST API `/api/v1/swap`
  - [ ] `simulate(params)` → 调用 REST API `/api/v1/simulate`
  - [ ] `getTokens()` → 调用 REST API `/api/v1/tokens`
- [ ] 类型定义导出（`QuoteResponse`、`SwapResponse`、`SimulationResponse`、`TokenInfo`）
- [ ] 错误处理和重试逻辑
- [ ] README 文档 + 使用示例
- [ ] 单元测试（mock HTTP）

**对应需求**: Req 6, Req 7  
**可复用代码**: 无（新增 TypeScript 层）

---

## Task 13: 前端 Demo（SvelteKit）

- [ ] 初始化 `apps/frontend`（SvelteKit + TypeScript + TailwindCSS）
- [ ] Swap 界面：
  - [ ] 代币选择器（input token / output token）
  - [ ] 金额输入
  - [ ] 报价展示（预期输出、最小输出、price impact、费用）
  - [ ] 路由可视化（显示路径经过哪些 DEX，拆单比例）
  - [ ] 滑点设置（0.1% / 0.5% / 1% / 自定义）
- [ ] 钱包集成：
  - [ ] Freighter 钱包连接
  - [ ] 交易签名和提交
  - [ ] 交易状态追踪
- [ ] 对比展示：聚合器输出 vs 各单独 DEX 输出
- [ ] 响应式设计（移动端适配）

**对应需求**: Req 10  
**可复用代码**: 无（新增前端）

---

## Task 14: 集成测试和端到端验证

- [ ] Testnet 环境搭建：
  - [ ] 部署聚合合约到 Testnet
  - [ ] 配置各 DEX 适配器连接 Testnet
- [ ] 端到端测试场景：
  - [ ] 单路径 swap（Soroswap USDC→XLM）
  - [ ] 多跳路径（USDC→XLM→ETH via Aquarius）
  - [ ] 拆单场景（大额 USDC→XLM 跨 Soroswap + Aquarius）
  - [ ] Classic DEX 路径（PathPayment）
  - [ ] 滑点保护触发（模拟价格变动）
  - [ ] 模拟失败处理
- [ ] 性能验证：报价计算 < 3s、API 响应 < 5s
- [ ] 错误场景测试：DEX 源超时、无可用路径、无效输入

**对应需求**: 全部需求的验收  
**可复用代码**: 无

---

## Task 15: 文档和 SCF 提交准备

- [ ] README.md：项目介绍、架构图、快速开始
- [ ] API 文档（OpenAPI/Swagger）
- [ ] SDK 文档（TypeDoc）
- [ ] 部署指南（如何运行路由引擎 + 前端）
- [ ] SCF Interest Form 内容准备：
  - [ ] 项目描述和价值主张
  - [ ] 技术架构概述
  - [ ] 里程碑和预算规划
  - [ ] 团队介绍（你的套利程序经验）
  - [ ] 与 StellarBroker 的差异化说明
- [ ] Demo 视频录制

**对应需求**: SCF 申请  
**可复用代码**: 无

---

## 建议开发顺序

```
Phase 1 (Week 1-2): 基础 + 核心路由
  Task 1 → Task 2 → Task 3 → Task 4 → Task 6 → Task 7

Phase 2 (Week 3-4): 交易执行 + API
  Task 9 → Task 8 → Task 10

Phase 3 (Week 5): 合约 + SDK
  Task 11 → Task 12 → Task 5

Phase 4 (Week 6): 前端 + 集成
  Task 13 → Task 14 → Task 15
```

Phase 1 完成后即可提交 SCF Interest Form（带 MVP demo）。


## 待完成任务 (TODO)

### 高优先级

- [x] **Sushi V3 (CLMM) 报价逻辑** — 使用 simulateTransaction 调用 Router 的 swap_exact_input_hints 做黑盒报价
  - Router: `CDMIM23WOUL5CZBKX3GOA3V5R5AMVIMTCP52KCDQORWELAPLJ27WZCHL`
  - Factory: `CD3KRKGDRVWPXVB3VXLUMQKMX6XZ6Q2H334IVZD4XXNAMKSRVQL5GLYF`
  - 通过 Factory get_pool() 发现池子，通过 simulate swap_exact_input_hints 获取报价

- [ ] **CLMM 本地报价（Sushi V3 + Aquarius Concentrate）** — 读取 tick 数据，本地遍历计算
  - 需要找到 Sushi 的合约源码理解 storage layout
  - Aquarius 也有 concentrate 池子（CLMM），同样需要 tick-based 报价
  - 两者可能共用同一套 tick math（Uniswap V3 风格）
  - 当前 Sushi 用 simulate 兜底（慢但能用），Aquarius concentrate 未接入

- [ ] **交易执行 (Transaction Builder)** — 生成可签名的 swap 交易 XDR
  - 单路径 Soroban swap: 调用聚合合约
  - Classic DEX: PathPaymentStrictSend
  - 混合路径: Classic + Soroban 在同一笔 tx

- [ ] **前端部署到 Cloudflare Pages** — 绑定 lumagg.xyz 域名

### 中优先级

- [ ] **缓存启动时的报价优化** — 当前从缓存启动后需要等 adapter 注册才能报价（已部分修复，local_quote fallback）
- [ ] **Aquarius stable swap 池子识别** — 当前用合约地址匹配 stablecoin，需要更准确的方式
- [ ] **前端 UI 打磨** — 代币图标（真实图片）、USD 价格显示、余额显示、交易历史
- [ ] **更多代币支持** — 从链上动态发现代币列表 + symbol 解析

### 低优先级

- [ ] **聚合合约部署到 mainnet** — 当前合约代码就绪，需要编译 WASM + 部署
- [ ] **SDK npm 包发布** — `@stellar-dex-aggregator/sdk`
- [ ] **性能监控** — 报价延迟、RPC 调用次数、缓存命中率
