//! Path-constrained split verification: re-quote each split leg through fixed
//! pools/paths using the same router-engine + Redis pool state as production.
//!
//! Usage:
//!   SNAPSHOT_REDIS_URL=redis://127.0.0.1:6379/ \
//!     cargo run -p api-server --release --bin verify-split-quote
//!
//!   API_URL=https://api.lumagg.xyz \
//!   AMOUNT_IN=10000000000000 \
//!   SNAPSHOT_REDIS_URL=redis://... \
//!     cargo run -p api-server --release --bin verify-split-quote
//!
//! Optional:
//!   LOCAL_QUOTE=1          — skip API; run full local split quote instead
//!   QUOTE_RPC_HYDRATE=1    — RPC fallback for xy=k Redis misses (like API env)

use {
    anyhow::{bail, Context, Result},
    api_server::{
        config::AppConfig,
        pool_hydrate::{self, PoolHydrateConfig},
        snapshot_loader::build_engine_from_snapshot,
    },
    dex_adapters::{classic_dex::ClassicDexAdapter, rpc::SorobanRpc},
    market_snapshot::{
        pool_state_store::{build_pool_state_store, PoolStateStore},
        store::{build_snapshot_store, SnapshotStoreBackend},
    },
    router_engine::{Path, QuoteEngine, RouteRequest, TokenId},
    serde::Deserialize,
    std::sync::Arc,
};

const DEFAULT_USDC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
const DEFAULT_XLM: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";

#[derive(Debug, Deserialize)]
struct ApiQuoteEnvelope {
    success: bool,
    data: Option<ApiQuoteData>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiQuoteData {
    amount_in: String,
    expected_output: String,
    price_impact: f64,
    sub_routes: Vec<ApiSubRoute>,
}

#[derive(Debug, Deserialize)]
struct ApiSubRoute {
    source: String,
    path: Vec<String>,
    pool_addresses: Vec<String>,
    dex_types: Vec<String>,
    amount_in: String,
    amount_out: String,
}

struct LegPlan {
    label: String,
    path: Path,
    amount_in: u128,
    expected_out: u128,
}

fn diff_bps(expected: u128, actual: u128) -> f64 {
    if expected == 0 {
        return if actual == 0 { 0.0 } else { 10_000.0 };
    }
    ((actual as f64 - expected as f64).abs() / expected as f64) * 10_000.0
}

fn rate(out: u128, input: u128) -> f64 {
    if input == 0 {
        return 0.0;
    }
    out as f64 / input as f64
}

fn path_from_sub_route(route: &ApiSubRoute) -> Result<Path> {
    if route.dex_types.len() != route.pool_addresses.len() {
        bail!(
            "dex_types/pool_addresses length mismatch for {}: {} vs {}",
            route.source,
            route.dex_types.len(),
            route.pool_addresses.len()
        );
    }
    if route.path.len() != route.pool_addresses.len() + 1 {
        bail!(
            "path token count mismatch for {}: {} tokens, {} pools",
            route.source,
            route.path.len(),
            route.pool_addresses.len()
        );
    }
    Ok(Path {
        tokens: route.path.iter().map(|t| TokenId::from_str_auto(t)).collect(),
        sources: route.dex_types.clone(),
        pool_addresses: route.pool_addresses.clone(),
        hops: route.pool_addresses.len(),
    })
}

async fn fetch_api_quote(api_url: &str, token_in: &str, token_out: &str, amount_in: u128) -> Result<ApiQuoteData> {
    let url = format!(
        "{}/api/v1/quote?token_in={token_in}&token_out={token_out}&amount_in={amount_in}&slippage=0.5&debug=1",
        api_url.trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .user_agent("lumagg-verify-split/1.0")
        .build()?;
    let body: ApiQuoteEnvelope = client.get(url).send().await?.json().await?;
    if !body.success {
        bail!("API quote failed: {}", body.error.unwrap_or_else(|| "unknown".into()));
    }
    body.data.context("API returned success but no data")
}

fn legs_from_quote(data: &ApiQuoteData) -> Result<Vec<LegPlan>> {
    data.sub_routes
        .iter()
        .enumerate()
        .map(|(i, route)| {
            Ok(LegPlan {
                label: format!("leg{} {}", i + 1, route.source),
                path: path_from_sub_route(route)?,
                amount_in: route.amount_in.parse()?,
                expected_out: route.amount_out.parse()?,
            })
        })
        .collect()
}

async fn build_local_engine(
    config: &AppConfig,
) -> Result<(Arc<QuoteEngine>, Arc<SorobanRpc>, Arc<dyn PoolStateStore>)> {
    let redis_url = config
        .snapshot_redis_url
        .as_deref()
        .context("SNAPSHOT_REDIS_URL (or REDIS_URL) is required for local verification")?;

    let snapshot_store = build_snapshot_store(SnapshotStoreBackend::Redis, None, Some(redis_url), None, None)?;
    let snapshot = snapshot_store.load_current_snapshot().await?;
    println!(
        "Loaded snapshot version {} (generated_at_ms={})",
        snapshot.version, snapshot.generated_at_ms
    );

    let engine = Arc::new(build_engine_from_snapshot(config, &snapshot).await?);
    engine.register_adapter(Arc::new(ClassicDexAdapter::new(None))).await;

    let rpc = Arc::new(SorobanRpc::new(&config.rpc_url, &config.network_passphrase));
    let pool_store = Arc::new(build_pool_state_store(redis_url)?);
    Ok((engine, rpc, pool_store))
}

async fn hydrate_for_paths(
    engine: &QuoteEngine,
    paths: &[Path],
    pool_store: &dyn PoolStateStore,
    rpc: &SorobanRpc,
) -> router_engine::QuoteHydration {
    let rpc_hydrate = std::env::var("QUOTE_RPC_HYDRATE")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let config = PoolHydrateConfig {
        rpc_hydrate_enabled: rpc_hydrate,
        ..PoolHydrateConfig::default()
    };
    let (hydration, redis_miss, soroswap_refs, _oldest_age_ms) =
        pool_hydrate::hydrate_paths(engine, paths, pool_store, rpc, &config).await;
    engine.set_aquarius_pools(hydration.aquarius_pools.clone()).await;
    println!(
        "Hydrated pools: xyk={} clmm={} aquarius={} comet={} (redis_miss_xyk={}/{} rpc_hydrate={})",
        hydration.xyk_pools.len(),
        hydration.clmm_pools.len(),
        hydration.aquarius_pools.len(),
        hydration.comet_pools.len(),
        redis_miss,
        soroswap_refs,
        rpc_hydrate
    );
    hydration
}

async fn quote_split_locally(
    engine: &QuoteEngine,
    pool_store: &dyn PoolStateStore,
    rpc: &SorobanRpc,
    token_in: &str,
    token_out: &str,
    amount_in: u128,
) -> Result<(u128, u128, f64, Vec<LegPlan>)> {
    let request = RouteRequest {
        token_in: TokenId::from_str_auto(token_in),
        token_out: TokenId::from_str_auto(token_out),
        amount_in,
        slippage_bps: Some(50),
        max_hops: None,
        max_splits: None,
        prefer_soroban: None,
    };
    let paths = engine.find_candidate_paths(&request).await;
    let hydration = hydrate_for_paths(engine, &paths, pool_store, rpc).await;
    let route = engine.get_route_with_paths(&request, &paths, Some(&hydration)).await;
    if route.sub_orders.is_empty() {
        bail!("local quote returned no route");
    }
    let legs: Vec<LegPlan> = route
        .sub_orders
        .iter()
        .enumerate()
        .map(|(i, so)| LegPlan {
            label: format!("leg{} {}", i + 1, so.path.sources.join(" → ")),
            path: so.path.clone(),
            amount_in: so.amount_in,
            expected_out: so.expected_amount_out,
        })
        .collect();
    Ok((
        route.total_amount_in,
        route.total_expected_out,
        route.price_impact_bps as f64 / 100.0,
        legs,
    ))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "api_server=warn,router_engine=warn".into()),
        )
        .init();

    let api_url = std::env::var("API_URL").unwrap_or_else(|_| "https://api.lumagg.xyz".to_string());
    let token_in = std::env::var("TOKEN_IN").unwrap_or_else(|_| DEFAULT_USDC.to_string());
    let token_out = std::env::var("TOKEN_OUT").unwrap_or_else(|_| DEFAULT_XLM.to_string());
    let amount_in: u128 = std::env::var("AMOUNT_IN")
        .unwrap_or_else(|_| "10000000000000".to_string())
        .parse()
        .context("invalid AMOUNT_IN")?;

    let mut config = AppConfig::from_env();
    if config.snapshot_redis_url.is_none() {
        config.snapshot_redis_url = std::env::var("REDIS_URL").ok();
    }

    let local_only = std::env::var("LOCAL_QUOTE")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let (engine, rpc, pool_store) = build_local_engine(&config).await?;

    let (total_in, total_out, price_impact, legs) = if local_only {
        quote_split_locally(&engine, pool_store.as_ref(), &rpc, &token_in, &token_out, amount_in).await?
    } else {
        let data = fetch_api_quote(&api_url, &token_in, &token_out, amount_in).await?;
        (
            data.amount_in.parse()?,
            data.expected_output.parse()?,
            data.price_impact,
            legs_from_quote(&data)?,
        )
    };

    let usdc = total_in as f64 / 1e7;
    let xlm = total_out as f64 / 1e7;
    let blended = rate(total_out, total_in);
    println!("\n=== Split plan ({:.0} USDC → {:.0} XLM) ===", usdc, xlm);
    println!(
        "Source: {} | blended {:.4} XLM/USDC | impact {:.2}% | {} legs",
        if local_only {
            "local engine".to_string()
        } else {
            api_url.clone()
        },
        blended,
        price_impact,
        legs.len()
    );

    let paths: Vec<Path> = legs.iter().map(|l| l.path.clone()).collect();
    let hydration = hydrate_for_paths(&engine, &paths, pool_store.as_ref(), &rpc).await;

    println!("\n=== Path-constrained re-quote (fixed pool/path per leg) ===");
    println!(
        "{:<28} {:>10} {:>8} {:>8} {:>8} {:>7} {}",
        "Leg", "USDC", "Split", "Exact", "Δ bps", "Match", "Pools"
    );
    println!("{}", "-".repeat(96));

    let mut sum_exact = 0u128;
    let mut all_ok = true;

    for leg in &legs {
        let Some(quote) = engine
            .quote_path_at_amount(&leg.path, leg.amount_in, Some(&hydration))
            .await
        else {
            all_ok = false;
            let pool_hint = leg
                .path
                .pool_addresses
                .first()
                .map(|p| &p[..p.len().min(12)])
                .unwrap_or("");
            println!(
                "{:<28} {:>10.0} {:>8.4} {:>8} {:>8} {:>7} FAILED (no quote) {}",
                leg.label,
                leg.amount_in as f64 / 1e7,
                rate(leg.expected_out, leg.amount_in),
                "-",
                "-",
                "NO",
                pool_hint
            );
            continue;
        };

        sum_exact += quote.amount_out;
        let split_rate = rate(leg.expected_out, leg.amount_in);
        let exact_rate = rate(quote.amount_out, leg.amount_in);
        let delta = diff_bps(leg.expected_out, quote.amount_out);
        let ok = delta < 1.0;
        if !ok {
            all_ok = false;
        }
        let pools = leg
            .path
            .pool_addresses
            .iter()
            .map(|p| p.get(..8).unwrap_or(p))
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{:<28} {:>10.0} {:>8.4} {:>8.4} {:>8.2} {:>7} {}",
            leg.label,
            leg.amount_in as f64 / 1e7,
            split_rate,
            exact_rate,
            delta,
            if ok { "OK" } else { "DIFF" },
            pools
        );
    }

    let total_delta_bps = diff_bps(total_out, sum_exact);
    let sum_legs: u128 = legs.iter().map(|l| l.expected_out).sum();
    println!("\n=== Totals ===");
    println!(
        "Split legs sum: {:.2} XLM | matches header: {}",
        sum_legs as f64 / 1e7,
        sum_legs == total_out
    );
    println!(
        "Exact path-constrained sum: {:.2} XLM | blended {:.4} | vs split header Δ {:.2} bps",
        sum_exact as f64 / 1e7,
        rate(sum_exact, total_in),
        total_delta_bps
    );

    if all_ok && total_delta_bps < 1.0 {
        println!("\n✓ All split legs match exact path-constrained quotes (within 1 bps).");
    } else if total_delta_bps < 5.0 {
        println!(
            "\n~ Mostly aligned (total Δ {:.2} bps). Residual drift may be API/snapshot timing.",
            total_delta_bps
        );
    } else {
        println!(
            "\n✗ Split header overstates output by {:.2} bps vs exact per-leg math.",
            total_delta_bps
        );
        println!("  (Re-deploy api-server after quote-cache fix if legs still diverge.)");
    }

    Ok(())
}
