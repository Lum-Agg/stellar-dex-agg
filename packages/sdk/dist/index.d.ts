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
export interface TokenInfo {
    id: string;
    symbol: string;
    name: string;
    logo?: string;
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
}
/** @deprecated Use LumAggClient */
export declare class StellarAggregator extends LumAggClient {
    constructor(options: {
        apiUrl: string;
    });
}
