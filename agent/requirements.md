# Requirements Document

## Introduction

Stellar DEX Aggregator Router 是一个多源流动性聚合路由引擎，聚合 Stellar 生态中多个去中心化交易所（Soroswap、Aquarius、Stellar Classic DEX、Comet 等）的流动性，为用户提供最优交易路径和执行方案。用户输入交易对和数量后，路由引擎查询多个 DEX 报价，通过智能路由算法（包括拆单策略）找到最优执行路径，最终输出一笔原子交易。

本项目的目标是申请 Stellar SCF Build Award，交付物包括：路由引擎（核心）、Soroban 聚合合约（可选）、SDK/API、以及简单前端 Demo。

## Glossary

- **Router_Engine**: 路由引擎，核心组件，负责查询多个 DEX 流动性源、计算最优路径、生成交易方案
- **DEX_Source**: 流动性源，指被聚合的单个去中心化交易所（如 Soroswap、Aquarius、Stellar Classic DEX、Comet）
- **Quote**: 报价，某个 DEX_Source 对特定交易对和数量给出的预期输出金额
- **Route**: 路由路径，从输入代币到输出代币的一条或多条交易路径的组合
- **Split_Order**: 拆单，将一笔大额交易拆分到多个 DEX_Source 或多条路径执行，以获得更优总输出
- **Atomic_Transaction**: 原子交易，所有子交易要么全部成功要么全部失败的交易包
- **Aggregator_Contract**: 聚合合约，部署在 Soroban 上的智能合约，负责在链上执行拆单和多路径交易
- **Slippage_Tolerance**: 滑点容忍度，用户允许的实际成交价格与报价之间的最大偏差百分比
- **Path**: 单条交易路径，由一系列中间代币跳转组成（如 USDC → XLM → ETH）
- **Hop**: 跳转，路径中的单次代币兑换操作
- **SDK**: 软件开发工具包，提供给第三方开发者集成聚合路由功能的程序库
- **Price_Impact**: 价格影响，交易量对市场价格造成的偏移程度

## Requirements

### Requirement 1: 多源流动性查询

**User Story:** As a trader, I want the router to query multiple DEX sources simultaneously, so that I can access the best available liquidity across the Stellar ecosystem.

#### Acceptance Criteria

1. WHEN a trade request is submitted, THE Router_Engine SHALL query all registered DEX_Source instances for the specified trading pair
2. THE Router_Engine SHALL support at minimum the following DEX_Source types: Soroswap (Soroban AMM), Aquarius (Soroban AMM), Stellar Classic DEX (orderbook), and Comet (Soroban weighted pool)
3. WHEN a DEX_Source fails to respond within 5 seconds, THE Router_Engine SHALL exclude that DEX_Source from the current routing calculation and proceed with available sources
4. IF a DEX_Source returns an invalid or malformed Quote, THEN THE Router_Engine SHALL discard that Quote and log the error
5. THE Router_Engine SHALL normalize all Quote responses into a unified format containing: output amount, price impact, and fee information

### Requirement 2: 最优路径计算

**User Story:** As a trader, I want the router to find the optimal trading path, so that I receive the maximum output amount for my trade.

#### Acceptance Criteria

1. WHEN Quotes from all available DEX_Sources are collected, THE Router_Engine SHALL calculate the Route that maximizes the output amount for the given input
2. THE Router_Engine SHALL support multi-hop paths with a maximum of 4 Hops per Path
3. THE Router_Engine SHALL evaluate both direct paths (single hop) and indirect paths (multi-hop through intermediate tokens) for each trade request
4. WHEN multiple Paths yield similar output amounts (within 0.1% difference), THE Router_Engine SHALL prefer the Path with fewer Hops
5. THE Router_Engine SHALL complete path calculation within 3 seconds for any valid trade request

### Requirement 3: 拆单策略

**User Story:** As a trader with large orders, I want the router to split my order across multiple sources, so that I minimize price impact and get better overall execution.

#### Acceptance Criteria

1. WHEN a single-path execution would result in Price_Impact exceeding 1%, THE Router_Engine SHALL evaluate Split_Order strategies
2. THE Router_Engine SHALL calculate optimal split ratios across available DEX_Sources to maximize total output amount
3. THE Router_Engine SHALL support splitting a trade into a maximum of 5 sub-orders across different DEX_Sources or Paths
4. WHEN Split_Order is applied, THE Router_Engine SHALL ensure the sum of all sub-order outputs exceeds the best single-path output
5. THE Router_Engine SHALL present the Split_Order breakdown to the user, showing each sub-order's DEX_Source, amount, and expected output

### Requirement 4: 原子交易生成

**User Story:** As a trader, I want all parts of my routed trade to execute atomically, so that I don't end up with partial fills or stuck intermediate tokens.

#### Acceptance Criteria

1. THE Router_Engine SHALL generate a single Atomic_Transaction that encapsulates all operations in the calculated Route
2. WHEN a Route involves multiple DEX_Sources, THE Router_Engine SHALL compose all swap operations into one Stellar transaction envelope
3. IF any operation within the Atomic_Transaction fails, THEN THE entire transaction SHALL revert with no state changes
4. THE Router_Engine SHALL include Slippage_Tolerance protection in the generated Atomic_Transaction, defaulting to 0.5% if not specified by the user
5. WHEN the Route involves Soroban DEX_Sources, THE Router_Engine SHALL generate appropriate Soroban contract invocations within the transaction

### Requirement 5: 滑点保护

**User Story:** As a trader, I want slippage protection on my trades, so that I don't receive significantly less than the quoted amount.

#### Acceptance Criteria

1. THE Router_Engine SHALL accept a user-specified Slippage_Tolerance as a percentage value between 0.01% and 50%
2. WHEN generating the Atomic_Transaction, THE Router_Engine SHALL set minimum output amounts based on the Quote minus the Slippage_Tolerance
3. IF the actual execution output falls below the minimum output amount, THEN THE Atomic_Transaction SHALL fail and revert
4. THE Router_Engine SHALL display the minimum guaranteed output amount to the user before transaction submission
5. WHEN no Slippage_Tolerance is specified, THE Router_Engine SHALL apply a default value of 0.5%

### Requirement 6: 报价 API

**User Story:** As a developer, I want a programmatic API to get trade quotes, so that I can integrate the aggregator into my own applications.

#### Acceptance Criteria

1. THE SDK SHALL expose a quote endpoint that accepts: input token, output token, input amount, and optional Slippage_Tolerance
2. WHEN a quote request is received, THE SDK SHALL return: expected output amount, Route details, Price_Impact, estimated fees, and minimum output amount
3. THE SDK SHALL return quote responses within 5 seconds for any valid token pair
4. IF the requested token pair has no available liquidity across all DEX_Sources, THEN THE SDK SHALL return a clear error indicating no route is available
5. THE SDK SHALL support both TypeScript/JavaScript and REST API interfaces

### Requirement 7: 交易执行 API

**User Story:** As a developer, I want an API to execute trades through the aggregator, so that I can build trading applications on top of it.

#### Acceptance Criteria

1. THE SDK SHALL expose an execute endpoint that accepts: input token, output token, input amount, Slippage_Tolerance, and user's signing key reference
2. WHEN an execute request is received, THE SDK SHALL generate the optimal Atomic_Transaction and return the unsigned transaction XDR for user signing
3. THE SDK SHALL provide a transaction simulation result before requiring user signature
4. IF the simulated transaction fails, THEN THE SDK SHALL return the failure reason without submitting to the network
5. WHEN the user submits the signed transaction, THE SDK SHALL monitor the transaction status and return the final execution result

### Requirement 8: DEX 源适配器架构

**User Story:** As a maintainer, I want a pluggable adapter architecture for DEX sources, so that new DEX protocols can be added without modifying the core routing logic.

#### Acceptance Criteria

1. THE Router_Engine SHALL define a standard DEX_Source adapter interface that all liquidity sources implement
2. THE DEX_Source adapter interface SHALL include methods for: querying available pairs, getting quotes for a specific pair and amount, and generating swap operation parameters
3. WHEN a new DEX_Source adapter is registered, THE Router_Engine SHALL include it in subsequent routing calculations without code changes to the core engine
4. THE Router_Engine SHALL support hot-registration of new DEX_Source adapters at runtime
5. EACH DEX_Source adapter SHALL encapsulate protocol-specific logic including: contract addresses, invocation parameters, and fee structures

### Requirement 9: Soroban 聚合合约

**User Story:** As a trader, I want an on-chain aggregator contract, so that split orders across Soroban DEXes can be executed atomically within a single contract call.

#### Acceptance Criteria

1. THE Aggregator_Contract SHALL accept a list of swap operations and execute them sequentially within a single invocation
2. WHEN any swap operation in the list fails or produces output below the specified minimum, THE Aggregator_Contract SHALL revert the entire invocation
3. THE Aggregator_Contract SHALL support swap operations targeting Soroswap, Aquarius, and Comet protocols
4. THE Aggregator_Contract SHALL verify that the final output token balance meets or exceeds the user-specified minimum output amount
5. THE Aggregator_Contract SHALL emit events for each executed swap operation containing: DEX_Source identifier, input amount, and output amount

### Requirement 10: 前端 Demo

**User Story:** As a grant reviewer, I want a functional frontend demo, so that I can evaluate the aggregator's user experience and capabilities.

#### Acceptance Criteria

1. THE Frontend_Demo SHALL provide a swap interface where users can select input token, output token, and input amount
2. WHEN a user inputs trade parameters, THE Frontend_Demo SHALL display the best Route including: expected output, Price_Impact, route visualization, and fee breakdown
3. WHEN Split_Order is recommended, THE Frontend_Demo SHALL visualize the order split across multiple DEX_Sources
4. THE Frontend_Demo SHALL integrate with Freighter or Albedo wallet for transaction signing
5. THE Frontend_Demo SHALL display a comparison showing the aggregator's output versus individual DEX_Source outputs

### Requirement 11: 路径发现

**User Story:** As a trader, I want the router to discover all possible trading paths, so that no profitable route is missed.

#### Acceptance Criteria

1. THE Router_Engine SHALL maintain a token graph representing all tradeable pairs across all registered DEX_Sources
2. WHEN a trade request is received, THE Router_Engine SHALL use breadth-first search to discover all valid Paths up to the maximum hop limit
3. THE Router_Engine SHALL update the token graph when DEX_Source liquidity pools are added or removed
4. THE Router_Engine SHALL support a configurable set of intermediate tokens (bridge tokens) used for multi-hop path discovery
5. WHEN the token graph is updated, THE Router_Engine SHALL invalidate cached paths that include affected pairs

### Requirement 12: 交易模拟

**User Story:** As a trader, I want to simulate my trade before execution, so that I can verify the expected outcome and avoid unexpected losses.

#### Acceptance Criteria

1. WHEN a Route is calculated, THE Router_Engine SHALL simulate the Atomic_Transaction using Stellar's transaction simulation endpoint
2. THE Router_Engine SHALL return simulation results including: actual output amount, resource costs (fees), and success/failure status
3. IF simulation reveals the output amount differs from the Quote by more than the Slippage_Tolerance, THEN THE Router_Engine SHALL warn the user and suggest re-quoting
4. THE Router_Engine SHALL use simulation results to provide accurate gas fee estimates to the user
5. WHEN simulation fails, THE Router_Engine SHALL provide a human-readable error description explaining the failure reason
