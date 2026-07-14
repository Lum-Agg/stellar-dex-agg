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

export interface TokenInfo {
  id: string;
  symbol: string;
  name: string;
  logo?: string;
}

export class LumAggClient {
  private baseUrl: string;
  private apiKey?: string;

  constructor(options: ClientOptions) {
    this.baseUrl = options.apiUrl.replace(/\/$/, '');
    this.apiKey = options.apiKey;
  }

  private headers(json = false): Record<string, string> {
    const h: Record<string, string> = { Accept: 'application/json' };
    if (json) h['Content-Type'] = 'application/json';
    if (this.apiKey) h['X-API-Key'] = this.apiKey;
    return h;
  }

  async isHealthy(): Promise<boolean> {
    try {
      const resp = await fetch(`${this.baseUrl}/api/v1/health`, { headers: this.headers() });
      const json = await resp.json();
      return json.status === 'ok';
    } catch {
      return false;
    }
  }

  async listTokens(): Promise<TokenInfo[]> {
    const resp = await fetch(`${this.baseUrl}/api/v1/tokens`, { headers: this.headers() });
    const json = await resp.json();
    const rows = json.data ?? json.tokens ?? [];
    return rows.map((t: Record<string, string>) => ({
      id: t.id,
      symbol: t.symbol,
      name: t.name,
      logo: t.logo,
    }));
  }

  /** @deprecated alias */
  async getTokens(): Promise<TokenInfo[]> {
    return this.listTokens();
  }

  async quote(params: QuoteParams): Promise<QuoteResult> {
    const search = new URLSearchParams({
      token_in: params.tokenIn,
      token_out: params.tokenOut,
      amount_in: params.amountIn,
    });
    if (params.slippage !== undefined) search.set('slippage', String(params.slippage));
    if (params.preferSoroban) search.set('prefer_soroban', '1');

    const resp = await fetch(`${this.baseUrl}/api/v1/quote?${search}`, { headers: this.headers() });
    const json = await resp.json();
    if (!json.success) throw new Error(json.error || 'Quote failed');

    const d = json.data;
    return {
      tokenIn: params.tokenIn,
      tokenOut: params.tokenOut,
      amountIn: d.amount_in ?? params.amountIn,
      expectedOutput: d.expected_output,
      minimumOutput: d.minimum_output,
      priceImpact: d.price_impact,
      isSplit: d.is_split,
      subRoutes: (d.sub_routes || []).map(mapSubRoute),
      computeTimeMs: d.compute_time_ms ?? 0,
    };
  }

  /** @deprecated alias */
  async getQuote(params: QuoteParams): Promise<QuoteResult> {
    return this.quote(params);
  }

  async buildTx(params: BuildTxParams): Promise<BuildTxResult> {
    const body = {
      user_public_key: params.userPublicKey,
      token_in: params.tokenIn,
      token_out: params.tokenOut,
      amount_in: params.amountIn,
      min_amount_out: params.minAmountOut,
      sub_routes: params.subRoutes.map((sr) => ({
        amount_in: sr.amountIn,
        steps: sr.poolAddresses.map((pool, i) => ({
          dex_type: sr.dexTypes[i] ?? 'aquarius',
          pool_address: pool,
          token_in: sr.path[i] ?? params.tokenIn,
          token_out: sr.path[i + 1] ?? params.tokenOut,
          in_idx: sr.inIndices[i] ?? 0,
          out_idx: sr.outIndices[i] ?? 1,
        })),
      })),
    };

    const resp = await fetch(`${this.baseUrl}/api/v1/build_tx`, {
      method: 'POST',
      headers: this.headers(true),
      body: JSON.stringify(body),
    });
    const json = await resp.json();
    if (!json.success) throw new Error(json.error || 'build_tx failed');

    return {
      unsignedTxXdr: json.data.unsigned_tx_xdr,
      fee: json.data.fee,
      execution: json.data.execution,
      numOperations: json.data.num_operations,
    };
  }

  /** Quote then build_tx in one call. */
  async quoteAndBuild(
    quoteParams: QuoteParams & { userPublicKey: string }
  ): Promise<{ quote: QuoteResult; tx: BuildTxResult }> {
    const quote = await this.quote(quoteParams);
    const tx = await this.buildTx({
      userPublicKey: quoteParams.userPublicKey,
      tokenIn: quote.tokenIn,
      tokenOut: quote.tokenOut,
      amountIn: quote.amountIn,
      minAmountOut: quote.minimumOutput,
      subRoutes: quote.subRoutes,
    });
    return { quote, tx };
  }

  /** Public on-chain stats from analytics-indexer (Tranche 3). */
  async getStats(params: StatsParams = {}): Promise<StatsResult | string> {
    const search = new URLSearchParams();
    if (params.day) search.set('day', params.day);
    if (params.format === 'csv') search.set('format', 'csv');

    const qs = search.toString();
    const url = `${this.baseUrl}/api/v1/stats${qs ? `?${qs}` : ''}`;
    const resp = await fetch(url, { headers: this.headers() });

    if (params.format === 'csv') {
      if (!resp.ok) throw new Error(`stats csv: HTTP ${resp.status}`);
      return resp.text();
    }

    const json = await resp.json();
    if (!json.success) throw new Error(json.error || 'stats failed');
    const d = json.data;
    return {
      dbPath: d.db_path,
      invocationCount: d.invocation_count,
      cursorLedger: d.cursor_ledger,
      oldestCreatedAt: d.oldest_created_at,
      daily: (d.daily || []).map(mapDailyStats),
    };
  }
}

function mapDailyStats(raw: Record<string, unknown>): DailyStats {
  return {
    day: String(raw.day ?? ''),
    txCount: Number(raw.tx_count ?? 0),
    uniqueUsers: Number(raw.unique_users ?? 0),
    totalAmountIn: String(raw.total_amount_in ?? '0'),
    splitSwapCount: Number(raw.split_swap_count ?? 0),
    successCount: Number(raw.success_count ?? 0),
    failedCount: Number(raw.failed_count ?? 0),
    byFunction: raw.by_function as Record<string, number> | undefined,
    byDex: raw.by_dex as Record<string, number> | undefined,
  };
}

function mapSubRoute(raw: Record<string, unknown>): QuoteSubRoute {
  const poolAddresses =
    (raw.pool_addresses as string[]) ?? (raw.poolAddresses as string[]) ?? [];
  const n = poolAddresses.length;
  const pad = <T>(arr: T[] | undefined, fill: T): T[] => {
    const a = [...(arr ?? [])];
    while (a.length < n) a.push(fill);
    return a;
  };
  return {
    source: String(raw.source ?? ''),
    path: (raw.path as string[]) ?? [],
    poolAddresses,
    dexTypes: pad(raw.dex_types as string[] | undefined, 'aquarius'),
    inIndices: pad(raw.in_indices as number[] | undefined, 0),
    outIndices: pad(raw.out_indices as number[] | undefined, 1),
    amountIn: String(raw.amount_in ?? '0'),
    amountOut: String(raw.amount_out ?? '0'),
    percentage: Number(raw.percentage ?? 0),
  };
}

/** @deprecated Use LumAggClient */
export class StellarAggregator extends LumAggClient {
  constructor(options: { apiUrl: string }) {
    super({ apiUrl: options.apiUrl });
  }
}
