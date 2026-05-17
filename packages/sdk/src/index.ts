/**
 * Stellar DEX Aggregator SDK
 *
 * Provides a simple interface to get quotes and execute swaps
 * through the Stellar DEX Aggregator API.
 */

export interface QuoteParams {
  tokenIn: string;
  tokenOut: string;
  amountIn: string;
  slippage?: number; // percentage, e.g. 0.5 = 0.5%
}

export interface SubRoute {
  source: string;
  path: string[];
  amountIn: string;
  amountOut: string;
  percentage: number;
}

export interface QuoteResult {
  expectedOutput: string;
  minimumOutput: string;
  priceImpact: number;
  isSplit: boolean;
  subRoutes: SubRoute[];
  computeTimeMs: number;
}

export interface SwapParams {
  tokenIn: string;
  tokenOut: string;
  amountIn: string;
  slippage: number;
  userPublicKey: string;
}

export interface SwapResult {
  unsignedTxXdr: string;
  simulation: {
    success: boolean;
    actualOutput?: string;
    fee?: string;
    error?: string;
  };
  route: QuoteResult;
}

export interface TokenInfo {
  id: string;
  symbol: string;
  name: string;
}

export class StellarAggregator {
  private baseUrl: string;

  constructor(options: { apiUrl: string }) {
    this.baseUrl = options.apiUrl.replace(/\/$/, '');
  }

  /**
   * Get the best quote for a swap.
   */
  async getQuote(params: QuoteParams): Promise<QuoteResult> {
    const searchParams = new URLSearchParams({
      token_in: params.tokenIn,
      token_out: params.tokenOut,
      amount_in: params.amountIn,
    });
    if (params.slippage !== undefined) {
      searchParams.set('slippage', params.slippage.toString());
    }

    const resp = await fetch(`${this.baseUrl}/api/v1/quote?${searchParams}`);
    const json = await resp.json();

    if (!json.success) {
      throw new Error(json.error || 'Quote failed');
    }

    return {
      expectedOutput: json.data.expected_output,
      minimumOutput: json.data.minimum_output,
      priceImpact: json.data.price_impact,
      isSplit: json.data.is_split,
      subRoutes: json.data.sub_routes.map((sr: any) => ({
        source: sr.source,
        path: sr.path,
        amountIn: sr.amount_in,
        amountOut: sr.amount_out,
        percentage: sr.percentage,
      })),
      computeTimeMs: json.data.compute_time_ms,
    };
  }

  /**
   * Build an unsigned swap transaction.
   * The returned XDR needs to be signed by the user's wallet (e.g., Freighter).
   */
  async buildSwap(params: SwapParams): Promise<SwapResult> {
    const resp = await fetch(`${this.baseUrl}/api/v1/swap`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        token_in: params.tokenIn,
        token_out: params.tokenOut,
        amount_in: params.amountIn,
        slippage: params.slippage,
        user_public_key: params.userPublicKey,
      }),
    });

    const json = await resp.json();

    if (!json.success) {
      throw new Error(json.error || 'Swap build failed');
    }

    return {
      unsignedTxXdr: json.data.unsigned_tx_xdr,
      simulation: {
        success: json.data.simulation.success,
        actualOutput: json.data.simulation.actual_output,
        fee: json.data.simulation.fee,
        error: json.data.simulation.error,
      },
      route: {
        expectedOutput: json.data.route.expected_output,
        minimumOutput: json.data.route.minimum_output,
        priceImpact: json.data.route.price_impact,
        isSplit: json.data.route.is_split,
        subRoutes: json.data.route.sub_routes || [],
        computeTimeMs: json.data.route.compute_time_ms || 0,
      },
    };
  }

  /**
   * Get list of supported tokens.
   */
  async getTokens(): Promise<TokenInfo[]> {
    const resp = await fetch(`${this.baseUrl}/api/v1/tokens`);
    const json = await resp.json();
    return json.tokens || [];
  }

  /**
   * Health check.
   */
  async isHealthy(): Promise<boolean> {
    try {
      const resp = await fetch(`${this.baseUrl}/api/v1/health`);
      const json = await resp.json();
      return json.status === 'ok';
    } catch {
      return false;
    }
  }
}
