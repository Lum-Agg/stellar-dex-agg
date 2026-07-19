/**
 * LumAgg TypeScript SDK — quote + build_tx (production REST surface).
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
    listSwaps(params: ListSwapsParams): Promise<SwapRecord[]>;
    getPrices(ids: string[]): Promise<PriceQuote[]>;
    getPriceHistory(id: string, range?: '24h' | '7d'): Promise<PricePoint[]>;
    /** Public on-chain stats from analytics-indexer (Tranche 3). */
    getStats(params?: StatsParams): Promise<StatsResult | string>;
}
/** @deprecated Use LumAggClient */
export declare class StellarAggregator extends LumAggClient {
    constructor(options: {
        apiUrl: string;
    });
}
