/**
 * LumAgg TypeScript SDK — quote, build_tx, wallet helpers, submit/poll.
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
        return mapBuildTxResult(json.data);
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
    async listOrders(params) {
        const search = new URLSearchParams({ user: params.user });
        if (params.status !== undefined)
            search.set('status', params.status);
        const resp = await fetch(`${this.baseUrl}/api/v1/orders?${search}`, {
            headers: this.headers(),
        });
        const json = await resp.json();
        if (!json.success)
            throw new Error(json.error || 'listOrders failed');
        return (json.data?.orders || []).map((r) => ({
            orderId: Number(r.order_id ?? 0),
            owner: String(r.owner ?? ''),
            tokenIn: String(r.token_in ?? ''),
            tokenOut: String(r.token_out ?? ''),
            amountInInitial: r.amount_in_initial != null ? String(r.amount_in_initial) : undefined,
            amountInRemaining: String(r.amount_in_remaining ?? '0'),
            limitOutPerInE7: String(r.limit_out_per_in_e7 ?? '0'),
            expiresLedger: Number(r.expires_ledger ?? 0),
            status: String(r.status ?? ''),
            createdLedger: r.created_ledger != null ? Number(r.created_ledger) : undefined,
            updatedLedger: Number(r.updated_ledger ?? 0),
            createdAt: r.created_at != null ? Number(r.created_at) : undefined,
            updatedAt: Number(r.updated_at ?? 0),
        }));
    }
    async buildCreateOrder(params) {
        const body = {
            user: params.user,
            token_in: params.tokenIn,
            token_out: params.tokenOut,
            amount_in: params.amountIn,
            limit_out_per_in_e7: params.limitOutPerInE7,
            expires_ledger: params.expiresLedger,
        };
        const resp = await fetch(`${this.baseUrl}/api/v1/orders/build_create`, {
            method: 'POST',
            headers: this.headers(true),
            body: JSON.stringify(body),
        });
        const json = await resp.json();
        if (!json.success)
            throw new Error(json.error || 'buildCreateOrder failed');
        return mapBuildOrderTxResult(json.data);
    }
    async buildCancelOrder(params) {
        const body = {
            user: params.user,
            order_id: params.orderId,
        };
        const resp = await fetch(`${this.baseUrl}/api/v1/orders/build_cancel`, {
            method: 'POST',
            headers: this.headers(true),
            body: JSON.stringify(body),
        });
        const json = await resp.json();
        if (!json.success)
            throw new Error(json.error || 'buildCancelOrder failed');
        return mapBuildOrderTxResult(json.data);
    }
    async listSwaps(params) {
        const search = new URLSearchParams({ user: params.user });
        if (params.limit !== undefined)
            search.set('limit', String(params.limit));
        if (params.cursor)
            search.set('cursor', params.cursor);
        const resp = await fetch(`${this.baseUrl}/api/v1/swaps?${search}`, {
            headers: this.headers(),
        });
        const json = await resp.json();
        if (!json.success)
            throw new Error(json.error || 'listSwaps failed');
        const swaps = (json.data?.swaps || []).map((r) => ({
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
        const nextCursor = json.data?.next_cursor != null && String(json.data.next_cursor).length > 0
            ? String(json.data.next_cursor)
            : undefined;
        return { swaps, nextCursor };
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
    async getBalance(params) {
        const search = new URLSearchParams({
            account: params.account,
            token: params.token,
        });
        const resp = await fetch(`${this.baseUrl}/api/v1/balance?${search}`, {
            headers: this.headers(),
        });
        const json = await resp.json();
        if (!json.success)
            throw new Error(json.error || 'getBalance failed');
        return {
            balance: json.balance != null ? String(json.balance) : undefined,
            hasTrustline: typeof json.has_trustline === 'boolean' ? json.has_trustline : undefined,
        };
    }
    async getBalances(params) {
        const search = new URLSearchParams({ account: params.account });
        const resp = await fetch(`${this.baseUrl}/api/v1/balances?${search}`, {
            headers: this.headers(),
        });
        const json = await resp.json();
        if (!json.success)
            throw new Error(json.error || 'getBalances failed');
        return {
            account: String(json.account ?? params.account),
            scope: String(json.scope ?? 'common'),
            tokensQueried: Array.isArray(json.tokens_queried)
                ? json.tokens_queried.map((t) => String(t))
                : [],
            balances: (json.balances ?? {}),
            hasTrustline: (json.has_trustline ?? {}),
            updatedAtMs: Number(json.updated_at_ms ?? 0),
        };
    }
    async getAccount(params) {
        const search = new URLSearchParams({ account: params.account });
        const resp = await fetch(`${this.baseUrl}/api/v1/account?${search}`, {
            headers: this.headers(),
        });
        const json = await resp.json();
        if (!json.success)
            throw new Error(json.error || 'getAccount failed');
        if (json.sequence == null)
            throw new Error('getAccount: missing sequence');
        return { sequence: String(json.sequence) };
    }
    async getClassicAsset(params) {
        const search = new URLSearchParams({ contract: params.contract });
        const resp = await fetch(`${this.baseUrl}/api/v1/classic_asset?${search}`, {
            headers: this.headers(),
        });
        const json = await resp.json();
        if (!json.success)
            throw new Error(json.error || 'getClassicAsset failed');
        return {
            code: json.code != null ? String(json.code) : undefined,
            issuer: json.issuer != null ? String(json.issuer) : undefined,
        };
    }
    async getLatestLedger() {
        const resp = await fetch(`${this.baseUrl}/api/v1/ledger/latest`, {
            headers: this.headers(),
        });
        const json = await resp.json();
        if (!json.success)
            throw new Error(json.error || 'getLatestLedger failed');
        return { sequence: Number(json.sequence ?? 0) };
    }
    async submitTx(params) {
        const resp = await fetch(`${this.baseUrl}/api/v1/submit_tx`, {
            method: 'POST',
            headers: this.headers(true),
            body: JSON.stringify({ signed_tx_xdr: params.signedTxXdr }),
        });
        const json = await resp.json();
        if (!json.success)
            throw new Error(json.error || 'submitTx failed');
        const hash = String(json.hash ?? '');
        if (!hash)
            throw new Error('submitTx: missing hash');
        return {
            hash,
            status: json.status != null ? String(json.status) : undefined,
        };
    }
    async getTxStatus(params) {
        const search = new URLSearchParams({ hash: params.hash });
        const resp = await fetch(`${this.baseUrl}/api/v1/tx_status?${search}`, {
            headers: this.headers(),
        });
        const json = await resp.json();
        if (!json.success && json.error) {
            return {
                hash: json.hash != null ? String(json.hash) : params.hash,
                status: json.status != null ? String(json.status) : undefined,
                confirmed: Boolean(json.confirmed),
                error: String(json.error),
            };
        }
        return {
            hash: json.hash != null ? String(json.hash) : params.hash,
            status: json.status != null ? String(json.status) : undefined,
            confirmed: Boolean(json.confirmed),
            error: json.error != null ? String(json.error) : undefined,
        };
    }
    /** Poll `/tx_status` until SUCCESS, FAILED, or timeout. */
    async waitForTx(hash, opts = {}) {
        const timeoutMs = opts.timeoutMs ?? 60000;
        const intervalMs = opts.intervalMs ?? 1000;
        const start = Date.now();
        while (Date.now() - start < timeoutMs) {
            const st = await this.getTxStatus({ hash });
            if (st.confirmed || st.status === 'FAILED')
                return st;
            await new Promise((r) => setTimeout(r, intervalMs));
        }
        throw new Error(`waitForTx timeout after ${timeoutMs}ms (hash=${hash})`);
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
function mapBuildTxResult(raw) {
    return {
        unsignedTxXdr: String(raw.unsigned_tx_xdr ?? ''),
        fee: String(raw.fee ?? ''),
        execution: String(raw.execution ?? ''),
        numOperations: Number(raw.num_operations ?? 0),
        contract: raw.contract != null ? String(raw.contract) : undefined,
    };
}
function mapBuildOrderTxResult(raw) {
    const base = mapBuildTxResult(raw);
    return {
        ...base,
        contract: String(raw.contract ?? base.contract ?? ''),
    };
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
