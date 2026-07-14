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
