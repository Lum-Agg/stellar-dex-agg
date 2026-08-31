# LumAgg Tranche 2 Demo Video Script

Target length: approximately four to five minutes. Upload as an unlisted
YouTube video, Loom video, or public Google Drive video.

方括号中的内容是操作提示，不要朗读。

## 录制准备

录制前先准备以下页面和窗口：

1. `https://lumagg.xyz`
2. `https://lumagg.gitbook.io/`
3. `https://github.com/Lum-Agg/stellar-dex-agg`
4. 仓库根目录中的终端窗口
5. `docs/arb-evidence-snapshot.md` 中的一笔近期 Stellar Expert 交易

不要展示私钥、助记词、Redis 密码、Telegram Token 或生产环境的私有配置文件。
视频只需要展示公开文档、公开证据和未签名交易的构建过程。

## 只使用截图的录制方案

整个视频可以只使用截图完成。不需要等待新的套利机会，也不要执行真实钱包交易。
按照下面的顺序截取页面，然后在对应的英文口播播放时展示相应截图。

### 0:00-0:20 - 项目介绍

打开并截图：

`https://lumagg.xyz`

展示 LumAgg Logo、导航栏和公开应用。在播放项目介绍时保持这张截图。

### 0:20-1:20 - SDK 和 API 文档

打开并截图：

`https://www.npmjs.com/package/@lumagg/sdk`

展示包名和版本号 `0.3.0`。

然后打开并截图：

`https://github.com/Lum-Agg/stellar-dex-agg/tree/main/packages/sdk`

展示 SDK 源码和示例文件。不要打开或展示任何私密配置文件。

然后打开并截图：

`https://lumagg.gitbook.io/lumagg/integrate/api-reference`

展示 API 文档，以及 quote 和 build transaction 接口说明。

然后打开并截图：

`https://github.com/Lum-Agg/stellar-dex-agg/tree/main/docs/evidence/d7-reference-sdk`

展示包含 `Quote OK`、`build_tx OK` 和未签名 XDR 前缀的 `README.md`。
这张截图已经足够，录制时不需要重新运行命令。

### 1:20-2:35 - 原子套利运行栈

打开并截图：

`https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/arb-operator.md`

展示运行架构、caller accounts、simulation 和 submission 相关章节。

然后打开并截图：

`https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/arb-evidence-snapshot.md`

展示 Vault 地址、Aggregator 地址、成功交易列表和一条示例日志。
不要读取或展示生产环境私有配置。

从 evidence snapshot 中打开一笔公开交易，例如：

`https://stellar.expert/explorer/public/tx/01234644f4444a4742ad234f9f42ea676eb32989d7478b19c6e88a03a6c6482d`

展示交易成功状态、交易操作和 round-trip 路由。这证明套利交易确实在 Stellar
主网上执行过。

### 2:35-3:20 - 集成验证总结

打开并截图：

`https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/integrator-pilots.md`

展示两条集成验证路径。SDK 证据页面已经在第一个 deliverable 中展示过，
这里不需要重新打开；只需要说明第一条路径复用了刚才看到的 SDK 证据。

### 3:20-4:05 - REST 集成路径

从刚才的 `integrator-pilots.md` 页面继续，说明 REST 证据来自 Tranche 1
D2，在 Tranche 2 D7 中作为 Path B 复用。

然后打开并截图：

`https://github.com/Lum-Agg/stellar-dex-agg/tree/main/docs/evidence/d2-integrator-smoke`

展示公开的 `quote.json` 和 `build_resp.json` 证据文件。这是历史公开证据，
不要将其描述为新的 tester 或新的合作伙伴关系。

### 4:05-4:35 - 收尾和自托管

打开并截图：

`https://lumagg.gitbook.io/lumagg/`

展示文档分类和部署文档。最后打开并截图公开 GitHub 仓库：

`https://github.com/Lum-Agg/stellar-dex-agg`

截图之间使用简单的淡入淡出效果。每张截图保持 15 到 30 秒即可。
不需要展示鼠标移动、实时输入或真实交易。

## 发音

- LumAgg：读作 **Loom Agg**，类似“卢姆 艾格”
- SDK：逐字母读 **S D K**
- API：逐字母读 **A P I**
- XDR：逐字母读 **X D R**
- DEX：读作 **decks**
- Soroban：读作 **So-ro-ban**
- Tranche：读作 **trahnsh**
- Aquarius：读作 **A-quare-ee-us**

## 0:00-0:25 - Introduction

[操作：展示 LumAgg 网站或 GitHub 仓库。]

Hello. This video demonstrates LumAgg's Tranche Two deliverables for the
Stellar Community Fund Build Award number forty-four.

Tranche Two focuses on three areas: the TypeScript SDK, an atomic arbitrage
operator stack, and reproducible integration validation.

LumAgg is an open-source Stellar DEX aggregator. It routes swaps across
multiple Stellar liquidity venues and provides both a public API and
self-hosted binaries.

## 0:25-1:35 - TypeScript SDK

[操作：打开 NPM 包页面或 GitHub 中的 SDK 目录。]

The first deliverable is the published `@lumagg/sdk` package. The current
package version is zero point three point zero.

The SDK provides typed methods for the quote and build transaction endpoints.
It handles route data, split routes, and unsigned transaction XDR returned by
the API. The application remains responsible for wallet signing and final
transaction submission.

[操作：短暂打开 GitBook API reference 页面。]

The API reference and integration guide are published in the LumAgg GitBook.
They describe the quote parameters, build transaction request, token metadata,
wallet signing flow, and Soroban-only routing options.

[操作：打开终端，运行准备好的 SDK 示例；或者直接展示
`docs/evidence/d7-reference-sdk/README.md` 中已经提交的输出。]

This example uses a public Stellar account address. It does not use a secret
key and it does not submit a transaction.

[操作：展示包含 `Quote OK` 和 `build_tx OK` 的输出。]

The SDK first receives a quote. This example returns a split route with two
legs. It then calls `build_tx`, which returns a valid unsigned Soroban
transaction XDR. The application can pass that XDR to its wallet adapter.

## 1:35-2:55 - Atomic Arbitrage Operator

[操作：打开 `docs/arb-operator.md` 或 GitHub 上的 evidence snapshot。]

The second deliverable is LumAgg's atomic arbitrage operator stack.

The stack consists of the LumAgg Aggregator contract, an arb-only Vault, and a
Rust arbitrage bot. The bot reads shared market state, requests quotes, checks
the expected result with Soroban simulation, and submits only controlled
atomic round-trip transactions.

The Vault holds the trading principal. Caller accounts are used for execution
and transaction fees; they do not provide an independent withdrawal path for
the trading capital.

[操作：在证据文档中展示 Vault 和 Aggregator 的合约 ID。]

The mainnet Vault and Aggregator contract addresses are documented here. The
operator documentation also explains configuration, caller management,
simulation, submission controls, and the limitations of this arb-only design.

[操作：从 evidence snapshot 中打开一笔 Stellar Expert 交易。]

This is a public mainnet example of an atomic round trip. The transaction
starts and ends with the same asset, and the intermediate swaps execute across
Stellar DEX liquidity.

The public evidence snapshot contains successful mainnet arbitrage
transactions, with the latest transaction links shown in the document. The
number of opportunities and the surplus are market-dependent and are not
guaranteed returns.

## 2:55-3:55 - Third and Final Deliverable: Integration Validation

[操作：保持刚才的 SDK 证据截图，或切换到集成验证报告：
`https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/integrator-pilots.md`。]

The third and final deliverable is integrator integration validation.

The first integration path is the in-repository SDK reference client.

It completes the same quote-to-build flow that a wallet or dApp would use:
the application provides the input token, output token, amount, slippage, and
public account address. LumAgg returns the route and an unsigned transaction.

The integrator signs the transaction with its own wallet. LumAgg does not take
custody of the user's private key or require custody of user funds.

## 3:55-4:35 - Integration Path B: REST API

[操作：在 GitHub 中打开 `docs/evidence/d2-integrator-smoke/`。]

The second integration path uses the REST API directly, without the SDK. This
uses the external validation evidence originally collected for Tranche One;
for Tranche Two, it is reused as the second reproducible adoption path rather
than presented as a new tester or a new partnership.

An external tester followed the documented flow using a public Stellar account
address. The captured evidence includes a successful quote response and a
successful build transaction response containing an unsigned XDR.

This demonstrates that third-party applications can integrate LumAgg using
plain JavaScript, TypeScript, or any HTTP client. The integrator can use its
own wallet adapter and its preferred Stellar RPC for signing and submission.

## 4:35-4:55 - Closing

[操作：先打开 GitBook 首页并截图：
`https://lumagg.gitbook.io/`；然后打开 GitHub 仓库首页并截图：
`https://github.com/Lum-Agg/stellar-dex-agg`。最后停留在其中任意一个公开页面。]

This completes the Tranche Two demonstration: the published TypeScript SDK,
the atomic arbitrage operator stack, and two reproducible integration paths.

The source code, release binaries, API documentation, and evidence are public.
Developers can use the hosted API or deploy their own aggregator and atomic
arbitrage operator.

Thank you for reviewing LumAgg.

## Links to Show at the End

- Website: https://lumagg.xyz
- Documentation: https://lumagg.gitbook.io/
- GitHub: https://github.com/Lum-Agg/stellar-dex-agg
- NPM SDK: https://www.npmjs.com/package/@lumagg/sdk
- Arb evidence: https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/arb-evidence-snapshot.md
- SDK evidence: https://github.com/Lum-Agg/stellar-dex-agg/tree/main/docs/evidence/d7-reference-sdk
- REST evidence: https://github.com/Lum-Agg/stellar-dex-agg/tree/main/docs/evidence/d2-integrator-smoke
