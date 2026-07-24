/**
 * LumAgg TypeScript SDK — quote, build_tx, wallet helpers, submit/poll.
 */
export interface ClientOptions {
    apiUrl: string;
    /** Partner key for 60 req/s (optional). */
    apiKey?: string;
}
export interface QuoteParams {
    tokenIn: string;
    tokenOut: string;
    amountIn: string;
    slippage?: number;
    /** When true, exclude Classic SDEX paths. */
    preferSoroban?: boolean;
}
export interface QuoteSubRoute {
    source: string;
    path: string[];
    poolAddresses: string[];
    dexTypes: string[];
    inIndices: number[];
    outIndices: number[];
    amountIn: string;
    amountOut: string;
    percentage: number;
}
export interface QuoteResult {
    tokenIn: string;
    tokenOut: string;
    amountIn: string;
    expectedOutput: string;
    minimumOutput: string;
    priceImpact: number;
    isSplit: boolean;
    subRoutes: QuoteSubRoute[];
    computeTimeMs: number;
}
export interface BuildTxParams {
    userPublicKey: string;
    tokenIn: string;
    tokenOut: string;
    amountIn: string;
    minAmountOut: string;
    subRoutes: QuoteSubRoute[];
}
export interface BuildTxResult {
    unsignedTxXdr: string;
    fee: string;
    execution: string;
    numOperations: number;
    contract?: string;
}
/** Unsigned escrow invoke XDR (create/cancel limit orders). */
export type BuildOrderTxResult = BuildTxResult & {
    contract: string;
};
export interface OrderRecord {
    orderId: number;
    owner: string;
    tokenIn: string;
    tokenOut: string;
    amountInInitial?: string;
    amountInRemaining: string;
    limitOutPerInE7: string;
    expiresLedger: number;
    status: string;
    createdLedger?: number;
    updatedLedger: number;
    createdAt?: number;
    updatedAt: number;
}
export interface ListOrdersParams {
    user: string;
    status?: 'open' | 'all';
}
export interface BuildCreateOrderParams {
    user: string;
    tokenIn: string;
    tokenOut: string;
    amountIn: string;
    limitOutPerInE7: string;
    expiresLedger: number;
}
export interface BuildCancelOrderParams {
    user: string;
    orderId: number;
}
export interface DailyStats {
    day: string;
    txCount: number;
    uniqueUsers: number;
    totalAmountIn: string;
    splitSwapCount: number;
    successCount: number;
    failedCount: number;
    byFunction?: Record<string, number>;
    byDex?: Record<string, number>;
}
export interface StatsResult {
    dbPath: string;
    invocationCount: number;
    cursorLedger?: number;
    oldestCreatedAt?: number;
    daily: DailyStats[];
}
export interface StatsParams {
    /** UTC day YYYY-MM-DD; omit for full rollup. */
    day?: string;
    /** When `csv`, returns raw CSV string instead of parsed JSON. */
    format?: 'json' | 'csv';
}
export interface SwapRecord {
    txHash: string;
    ledger: number;
    createdAt: number;
    status: string;
    functionName: string;
    tokenIn?: string;
    tokenOut?: string;
    amountIn: string;
    amountOut?: string;
    isSplit: boolean;
}
export interface ListSwapsParams {
    user: string;
    limit?: number;
    /** Opaque cursor from a previous page (`nextCursor`). */
    cursor?: string;
}
export interface ListSwapsResult {
    swaps: SwapRecord[];
    nextCursor?: string;
}
export interface PriceQuote {
    id: string;
    priceUsdc: number;
    ts: number;
    via: string;
}
export interface PricePoint {
    ts: number;
    priceUsdc: number;
}
export interface BalanceResult {
    balance?: string;
    hasTrustline?: boolean;
}
export interface BalancesResult {
    account: string;
    scope: string;
    tokensQueried: string[];
    balances: Record<string, string>;
    hasTrustline: Record<string, boolean>;
    updatedAtMs: number;
}
export interface AccountResult {
    sequence: string;
}
export interface ClassicAssetResult {
    code?: string;
    issuer?: string;
}
export interface SubmitTxResult {
    hash: string;
    status?: string;
}
export interface TxStatusResult {
    hash?: string;
    status?: string;
    confirmed: boolean;
    error?: string;
}
export interface WaitForTxOptions {
    timeoutMs?: number;
    intervalMs?: number;
}
export interface TokenInfo {
    id: string;
    symbol: string;
    name: string;
    logo?: string;
    /** `"official"` for SEP-42 icons, `"fallback"` for generated letter avatars. */
    logoKind?: 'official' | 'fallback';
}
export declare class LumAggClient {
    private baseUrl;
    private apiKey?;
    constructor(options: ClientOptions);
    private headers;
    isHealthy(): Promise<boolean>;
    listTokens(): Promise<TokenInfo[]>;
    /** @deprecated alias */
    getTokens(): Promise<TokenInfo[]>;
    quote(params: QuoteParams): Promise<QuoteResult>;
    /** @deprecated alias */
    getQuote(params: QuoteParams): Promise<QuoteResult>;
    buildTx(params: BuildTxParams): Promise<BuildTxResult>;
    /** Quote then build_tx in one call. */
    quoteAndBuild(quoteParams: QuoteParams & {
        userPublicKey: string;
    }): Promise<{
        quote: QuoteResult;
        tx: BuildTxResult;
    }>;
    listOrders(params: ListOrdersParams): Promise<OrderRecord[]>;
    buildCreateOrder(params: BuildCreateOrderParams): Promise<BuildOrderTxResult>;
    buildCancelOrder(params: BuildCancelOrderParams): Promise<BuildOrderTxResult>;
    listSwaps(params: ListSwapsParams): Promise<ListSwapsResult>;
    getPrices(ids: string[]): Promise<PriceQuote[]>;
    getPriceHistory(id: string, range?: '24h' | '7d'): Promise<PricePoint[]>;
    getBalance(params: {
        account: string;
        token: string;
    }): Promise<BalanceResult>;
    getBalances(params: {
        account: string;
    }): Promise<BalancesResult>;
    getAccount(params: {
        account: string;
    }): Promise<AccountResult>;
    getClassicAsset(params: {
        contract: string;
    }): Promise<ClassicAssetResult>;
    getLatestLedger(): Promise<{
        sequence: number;
    }>;
    submitTx(params: {
        signedTxXdr: string;
    }): Promise<SubmitTxResult>;
    getTxStatus(params: {
        hash: string;
    }): Promise<TxStatusResult>;
    /** Poll `/tx_status` until SUCCESS, FAILED, or timeout. */
    waitForTx(hash: string, opts?: WaitForTxOptions): Promise<TxStatusResult>;
    /** Public on-chain stats from analytics-indexer (Tranche 3). */
    getStats(params?: StatsParams): Promise<StatsResult | string>;
}
/** @deprecated Use LumAggClient */
export declare class StellarAggregator extends LumAggClient {
    constructor(options: {
        apiUrl: string;
    });
}
