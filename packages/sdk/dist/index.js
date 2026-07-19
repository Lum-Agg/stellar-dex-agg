/**
 * LumAgg TypeScript SDK — quote + build_tx (production REST surface).
 */
export class LumAggClient {
    constructor(options) {
        this.baseUrl = options.apiUrl.replace(/\/$/, '');
        this.apiKey = options.apiKey;
    }
    headers(json = false) {
        const h = { Accept: 'application/json' };
        if (json)
            h['Content-Type'] = 'application/json';
        if (this.apiKey)
            h['X-API-Key'] = this.apiKey;
        return h;
    }
    async isHealthy() {
        try {
            const resp = await fetch(`${this.baseUrl}/api/v1/health`, { headers: this.headers() });
            const json = await resp.json();
            return json.status === 'ok';
        }
        catch {
            return false;
        }
    }
    async listTokens() {
        const resp = await fetch(`${this.baseUrl}/api/v1/tokens`, { headers: this.headers() });
        const json = await resp.json();
        const rows = json.data ?? json.tokens ?? [];
        return rows.map((t) => ({
            id: t.id,
            symbol: t.symbol,
            name: t.name,
            logo: t.logo,
            logoKind: t.logo_kind === 'official' || t.logo_kind === 'fallback' ? t.logo_kind : undefined,
        }));
    }
    /** @deprecated alias */
    async getTokens() {
        return this.listTokens();
    }
    async quote(params) {
        const search = new URLSearchParams({
            token_in: params.tokenIn,
            token_out: params.tokenOut,
            amount_in: params.amountIn,
        });
        if (params.slippage !== undefined)
            search.set('slippage', String(params.slippage));
        if (params.preferSoroban)
            search.set('prefer_soroban', '1');
        const resp = await fetch(`${this.baseUrl}/api/v1/quote?${search}`, { headers: this.headers() });
        const json = await resp.json();
        if (!json.success)
            throw new Error(json.error || 'Quote failed');
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
    async getQuote(params) {
        return this.quote(params);
    }
    async buildTx(params) {
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
        if (!json.success)
            throw new Error(json.error || 'build_tx failed');
        return {
            unsignedTxXdr: json.data.unsigned_tx_xdr,
            fee: json.data.fee,
            execution: json.data.execution,
            numOperations: json.data.num_operations,
        };
    }
    /** Quote then build_tx in one call. */
    async quoteAndBuild(quoteParams) {
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
    async listSwaps(params) {
        const search = new URLSearchParams({ user: params.user });
        if (params.limit !== undefined)
            search.set('limit', String(params.limit));
        const resp = await fetch(`${this.baseUrl}/api/v1/swaps?${search}`, {
            headers: this.headers(),
        });
        const json = await resp.json();
        if (!json.success)
            throw new Error(json.error || 'listSwaps failed');
        return (json.data?.swaps || []).map((r) => ({
            txHash: String(r.tx_hash ?? ''),
            ledger: Number(r.ledger ?? 0),
            createdAt: Number(r.created_at ?? 0),
            status: String(r.status ?? ''),
            functionName: String(r.function_name ?? ''),
            tokenIn: r.token_in != null ? String(r.token_in) : undefined,
            tokenOut: r.token_out != null ? String(r.token_out) : undefined,
            amountIn: String(r.amount_in ?? '0'),
            amountOut: r.amount_out != null ? String(r.amount_out) : undefined,
            isSplit: Boolean(r.is_split),
        }));
    }
    async getPrices(ids) {
        const search = new URLSearchParams({ ids: ids.join(',') });
        const resp = await fetch(`${this.baseUrl}/api/v1/prices?${search}`, {
            headers: this.headers(),
        });
        const json = await resp.json();
        if (!json.success)
            throw new Error(json.error || 'getPrices failed');
        return (json.data?.prices || []).map((r) => ({
            id: String(r.id ?? ''),
            priceUsdc: Number(r.price_usdc ?? 0),
            ts: Number(r.ts ?? 0),
            via: String(r.via ?? ''),
        }));
    }
    async getPriceHistory(id, range = '24h') {
        const search = new URLSearchParams({ id, range });
        const resp = await fetch(`${this.baseUrl}/api/v1/prices/history?${search}`, {
            headers: this.headers(),
        });
        const json = await resp.json();
        if (!json.success)
            throw new Error(json.error || 'getPriceHistory failed');
        return (json.data?.points || []).map((r) => ({
            ts: Number(r.ts ?? 0),
            priceUsdc: Number(r.price_usdc ?? 0),
        }));
    }
    /** Public on-chain stats from analytics-indexer (Tranche 3). */
    async getStats(params = {}) {
        const search = new URLSearchParams();
        if (params.day)
            search.set('day', params.day);
        if (params.format === 'csv')
            search.set('format', 'csv');
        const qs = search.toString();
        const url = `${this.baseUrl}/api/v1/stats${qs ? `?${qs}` : ''}`;
        const resp = await fetch(url, { headers: this.headers() });
        if (params.format === 'csv') {
            if (!resp.ok)
                throw new Error(`stats csv: HTTP ${resp.status}`);
            return resp.text();
        }
        const json = await resp.json();
        if (!json.success)
            throw new Error(json.error || 'stats failed');
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
function mapDailyStats(raw) {
    return {
        day: String(raw.day ?? ''),
        txCount: Number(raw.tx_count ?? 0),
        uniqueUsers: Number(raw.unique_users ?? 0),
        totalAmountIn: String(raw.total_amount_in ?? '0'),
        splitSwapCount: Number(raw.split_swap_count ?? 0),
        successCount: Number(raw.success_count ?? 0),
        failedCount: Number(raw.failed_count ?? 0),
        byFunction: raw.by_function,
        byDex: raw.by_dex,
    };
}
function mapSubRoute(raw) {
    const poolAddresses = raw.pool_addresses ?? raw.poolAddresses ?? [];
    const n = poolAddresses.length;
    const pad = (arr, fill) => {
        const a = [...(arr ?? [])];
        while (a.length < n)
            a.push(fill);
        return a;
    };
    return {
        source: String(raw.source ?? ''),
        path: raw.path ?? [],
        poolAddresses,
        dexTypes: pad(raw.dex_types, 'aquarius'),
        inIndices: pad(raw.in_indices, 0),
        outIndices: pad(raw.out_indices, 1),
        amountIn: String(raw.amount_in ?? '0'),
        amountOut: String(raw.amount_out ?? '0'),
        percentage: Number(raw.percentage ?? 0),
    };
}
/** @deprecated Use LumAggClient */
export class StellarAggregator extends LumAggClient {
    constructor(options) {
        super({ apiUrl: options.apiUrl });
    }
}
